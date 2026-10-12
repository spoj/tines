//! tines: keeps a Pi running on a machine, always. See README.md.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime};

use anyhow::{Context, Result, bail, ensure};
use serde::Deserialize;
use sha2::{Digest, Sha256};

/// How soon Pi starts again after it ends; each further run shorter than STEADY doubles it, up to LONGEST_WAIT.
const FIRST_WAIT: Duration = Duration::from_secs(2);
const LONGEST_WAIT: Duration = Duration::from_secs(60);
const STEADY: Duration = Duration::from_secs(60);

/// How often the keeper looks whether Pi ended, or whether it was told to stop.
const POLL: Duration = Duration::from_millis(250);

/// The size past which `keep.log` is moved aside to `keep.log.old` before Pi starts.
const LOG_LIMIT: u64 = 1 << 20;

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
    digest: String,
}

fn main() -> Result<()> {
    match std::env::args().nth(1).as_deref() {
        Some("update") => update(),
        Some("install") => install(),
        Some("uninstall") => service::uninstall(),
        Some("keep") => keep(),
        _ => bail!("usage: tines update | install | uninstall | keep"),
    }
}

/// Installs or updates tines in `~/.tines/bin`.
fn update() -> Result<()> {
    let bin = home()?.join("bin");
    std::fs::create_dir_all(&bin)?;
    // What an earlier update moved aside on Windows, unless it still runs.
    for entry in std::fs::read_dir(&bin)? {
        let path = entry?.path();
        if path.extension().is_some_and(|e| e == "old") {
            let _ = std::fs::remove_file(path);
        }
    }

    let release: Release = get("https://api.github.com/repos/spoj/tines/releases/latest")?;
    let running = env!("CARGO_PKG_VERSION");
    let latest = &release.tag_name[1..];
    if version(latest) > version(running) || !bin.join(exe("tines")).exists() {
        fetch(&release, &bin)?;
        println!("tines {running} → {latest}");
    } else {
        println!("tines {running} is current");
    }
    Ok(())
}

/// Keeps this machine's tine: Pi in RPC mode, as the session named `tine` with pi-letmeknow, which puts it in letmeknow
/// as `tine`, working in `~/.tines/tine`. When Pi
/// ends, the processes it started end too, and Pi starts again with a fresh context. Before each start, tines runs itself
/// anew if `tines update` replaced it. What happens, and what Pi writes to stderr, goes to `~/.tines/keep.log`.
fn keep() -> Result<()> {
    let home = home()?;
    let dir = home.join("tine");
    std::fs::create_dir_all(&dir)?;
    let me = std::env::current_exe()?;
    let built = std::fs::metadata(&me)?.modified()?;
    group::init();
    let mut short: u32 = 0;
    loop {
        if std::fs::metadata(&me)?.modified()? != built {
            log(&home, "tines was updated: running it anew")?;
            return renew(&me);
        }
        if std::fs::metadata(home.join("keep.log")).is_ok_and(|log| log.len() > LOG_LIMIT) {
            std::fs::rename(home.join("keep.log"), home.join("keep.log.old"))?;
        }
        log(&home, "starting pi")?;
        let started = Instant::now();
        let Some(ended) = run_pi(&home, &dir)? else { return log(&home, "stopped") };
        short = if started.elapsed() < STEADY { short + 1 } else { 0 };
        let wait = LONGEST_WAIT.min(FIRST_WAIT * 2u32.pow(short.saturating_sub(1).min(5)));
        log(&home, &format!("{ended}; starting it again in {} s", wait.as_secs()))?;
        let asleep = Instant::now();
        while asleep.elapsed() < wait && !group::stopping() {
            std::thread::sleep(POLL);
        }
        if group::stopping() {
            return log(&home, "stopped");
        }
    }
}

/// Has the system run `tines keep` from `~/.tines/bin` from now on, and again after it restarts, with the PATH this
/// command runs with, which Pi and its tools need.
fn install() -> Result<()> {
    let tines = home()?.join("bin").join(exe("tines"));
    ensure!(tines.exists(), "{} does not exist: run tines update first", tines.display());
    service::install(&tines, &std::env::var("PATH")?)
}

/// A systemd user service, which runs without a login once the user lingers.
#[cfg(target_os = "linux")]
mod service {
    use std::path::{Path, PathBuf};
    use std::process::Command;

    use anyhow::{Context, Result};

    pub fn install(tines: &Path, path: &str) -> Result<()> {
        let text = format!(
            "[Unit]\nDescription=tines: this machine's tine\n\n[Service]\nExecStart=\"{}\" keep\nEnvironment=\"PATH={path}\"\nRestart=always\nRestartSec=10\n\n[Install]\nWantedBy=default.target\n",
            tines.display()
        );
        let file = unit()?;
        std::fs::create_dir_all(file.parent().context("no parent")?)?;
        std::fs::write(&file, text)?;
        super::run(Command::new("systemctl").args(["--user", "daemon-reload"]))?;
        super::run(Command::new("systemctl").args(["--user", "enable", "tines"]))?;
        super::run(Command::new("systemctl").args(["--user", "restart", "tines"]))?;
        super::run(Command::new("loginctl").arg("enable-linger"))
    }

    pub fn uninstall() -> Result<()> {
        super::run(Command::new("systemctl").args(["--user", "disable", "--now", "tines"]))?;
        std::fs::remove_file(unit()?)?;
        super::run(Command::new("systemctl").args(["--user", "daemon-reload"]))
    }

    fn unit() -> Result<PathBuf> {
        let config = match std::env::var_os("XDG_CONFIG_HOME") {
            Some(config) => PathBuf::from(config),
            None => std::env::home_dir().context("no home directory")?.join(".config"),
        };
        Ok(config.join("systemd/user/tines.service"))
    }
}

/// A launchd agent of the user's, which runs while the user is logged in.
#[cfg(target_os = "macos")]
mod service {
    use std::path::{Path, PathBuf};
    use std::process::Command;

    use anyhow::{Context, Result};

    pub fn install(tines: &Path, path: &str) -> Result<()> {
        let log = super::home()?.join("keep.log");
        let text = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>tines</string>
  <key>ProgramArguments</key><array><string>{}</string><string>keep</string></array>
  <key>EnvironmentVariables</key><dict><key>PATH</key><string>{path}</string></dict>
  <key>RunAtLoad</key><true/>
  <key>KeepAlive</key><true/>
  <key>StandardErrorPath</key><string>{}</string>
</dict>
</plist>
"#,
            tines.display(),
            log.display()
        );
        let file = plist()?;
        std::fs::create_dir_all(file.parent().context("no parent")?)?;
        std::fs::write(&file, text)?;
        // An agent loaded already is replaced; one that is not makes bootout fail, which is fine.
        let _ = Command::new("launchctl").args(["bootout", &format!("{}/tines", domain())]).output();
        super::run(Command::new("launchctl").args(["bootstrap", &domain()]).arg(&file))
    }

    pub fn uninstall() -> Result<()> {
        super::run(Command::new("launchctl").args(["bootout", &format!("{}/tines", domain())]))?;
        Ok(std::fs::remove_file(plist()?)?)
    }

    fn domain() -> String {
        format!("gui/{}", unsafe { libc::getuid() })
    }

    fn plist() -> Result<PathBuf> {
        Ok(std::env::home_dir().context("no home directory")?.join("Library/LaunchAgents/tines.plist"))
    }
}

/// A program of the user's that Windows runs when the user logs in (the Run key), which needs no administrator. It takes
/// the user's own PATH, not this command's.
#[cfg(windows)]
mod service {
    use std::os::windows::process::CommandExt;
    use std::path::Path;
    use std::process::{Command, Stdio};

    use anyhow::Result;
    use windows_sys::Win32::Foundation::{HANDLE_FLAG_INHERIT, SetHandleInformation};
    use windows_sys::Win32::System::Console::{GetStdHandle, STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE};
    use windows_sys::Win32::System::Threading::{CREATE_BREAKAWAY_FROM_JOB, CREATE_NEW_PROCESS_GROUP, DETACHED_PROCESS};

    const RUN: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";

    pub fn install(tines: &Path, _path: &str) -> Result<()> {
        stop();
        super::run(Command::new("reg").args(["add", RUN, "/v", "tines", "/t", "REG_SZ", "/f", "/d"]).arg(format!("\"{}\" keep", tines.display())))?;
        // Detached from this console and its stdio, and out of the terminal's job where the job allows it, so that the
        // keeper outlives the terminal. Windows hands a child every inheritable handle, and a pipe reading this command's
        // output would stay open while the keeper runs, so this command's own stdio is no longer inheritable.
        for handle in [STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, STD_ERROR_HANDLE] {
            unsafe { SetHandleInformation(GetStdHandle(handle), HANDLE_FLAG_INHERIT, 0) };
        }
        let mut keep = Command::new(tines);
        keep.arg("keep").stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
        let flags = DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP;
        if keep.creation_flags(flags | CREATE_BREAKAWAY_FROM_JOB).spawn().is_err() {
            keep.creation_flags(flags).spawn()?;
        }
        Ok(())
    }

    pub fn uninstall() -> Result<()> {
        stop();
        super::run(Command::new("reg").args(["delete", RUN, "/v", "tines", "/f"]))
    }

    /// Ends the running keeper, if one runs, which ends its Pi: every tines but this one.
    fn stop() {
        let others = format!("PID ne {}", std::process::id());
        let _ = Command::new("taskkill").args(["/F", "/FI", "IMAGENAME eq tines.exe", "/FI", &others]).output();
    }
}

/// Runs Pi until it ends, then ends what it left running; how it ended, or None if tines was told to stop meanwhile.
fn run_pi(home: &Path, dir: &Path) -> Result<Option<String>> {
    let errors = OpenOptions::new().create(true).append(true).open(home.join("keep.log"))?;
    let mut command = group::pi();
    command.args(["--mode", "rpc", "--name", "tine", "-e", "git:github.com/spoj/pi-letmeknow"]).current_dir(dir);
    // RPC mode ends when its input does: the keeper holds it open, and writes nothing.
    command.stdin(Stdio::piped()).stdout(Stdio::null()).stderr(errors);
    let (mut pi, group) = match group::spawn(&mut command) {
        Ok(spawned) => spawned,
        Err(error) => return Ok(Some(format!("pi did not start: {error}"))),
    };
    let status = loop {
        if let Some(status) = pi.try_wait()? {
            break Some(status);
        }
        if group::stopping() {
            break None;
        }
        std::thread::sleep(POLL);
    };
    group.end(&mut pi);
    Ok(status.map(|status| format!("pi ended ({status})")))
}

/// Runs `me`, which `tines update` replaced, in place of this tines, with the same arguments.
fn renew(me: &Path) -> Result<()> {
    let mut command = Command::new(me);
    command.args(std::env::args_os().skip(1));
    #[cfg(unix)]
    return Err(std::os::unix::process::CommandExt::exec(&mut command).into());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let detached = windows_sys::Win32::System::Threading::DETACHED_PROCESS;
        command.creation_flags(detached).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn()?;
        Ok(())
    }
}

fn log(home: &Path, text: &str) -> Result<()> {
    let mut file = OpenOptions::new().create(true).append(true).open(home.join("keep.log"))?;
    Ok(writeln!(file, "{} {text}", humantime::format_rfc3339_seconds(SystemTime::now()))?)
}

/// Pi's processes, which end together: a process group of their own.
#[cfg(unix)]
mod group {
    use std::os::unix::process::CommandExt;
    use std::process::{Child, Command};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::{Duration, Instant};

    /// How long Pi's processes have to end once asked to, before they are killed.
    const GRACE: Duration = Duration::from_secs(5);

    static STOPPING: AtomicBool = AtomicBool::new(false);

    extern "C" fn stop(_: libc::c_int) {
        STOPPING.store(true, Ordering::SeqCst);
    }

    /// From now on SIGTERM, SIGINT and SIGHUP tell the keeper to stop, which it does once it ended Pi's processes.
    pub fn init() {
        for signal in [libc::SIGTERM, libc::SIGINT, libc::SIGHUP] {
            unsafe { libc::signal(signal, stop as extern "C" fn(libc::c_int) as libc::sighandler_t) };
        }
    }

    pub fn stopping() -> bool {
        STOPPING.load(Ordering::SeqCst)
    }

    pub fn pi() -> Command {
        Command::new("pi")
    }

    pub struct Group(libc::pid_t);

    pub fn spawn(command: &mut Command) -> std::io::Result<(Child, Group)> {
        let child = command.process_group(0).spawn()?;
        let id = child.id() as libc::pid_t;
        Ok((child, Group(id)))
    }

    impl Group {
        /// Asks every process of the group to end, Pi too if it still runs, and kills those left after GRACE.
        pub fn end(self, pi: &mut Child) {
            unsafe { libc::kill(-self.0, libc::SIGTERM) };
            let asked = Instant::now();
            // Pi counts until it is reaped.
            while unsafe { libc::kill(-self.0, 0) } == 0 && asked.elapsed() < GRACE {
                let _ = pi.try_wait();
                std::thread::sleep(super::POLL);
            }
            unsafe { libc::kill(-self.0, libc::SIGKILL) };
            let _ = pi.wait();
        }
    }
}

/// Pi's processes, which end together: a job, which ends them all once closed. Windows ends a process without telling
/// it, so the keeper is never told to stop; its job handle closes as it ends, which ends Pi's.
#[cfg(windows)]
mod group {
    use std::os::windows::io::AsRawHandle;
    use std::os::windows::process::CommandExt;
    use std::process::{Child, Command};
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::System::Console::{FreeConsole, GetConsoleProcessList};
    use windows_sys::Win32::System::Threading::CREATE_NO_WINDOW;
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JobObjectExtendedLimitInformation, SetInformationJobObject,
    };

    /// Lets go of a console that is the keeper's alone, as the scheduled task gives it, which closes its window.
    pub fn init() {
        let mut processes = [0u32; 2];
        if unsafe { GetConsoleProcessList(processes.as_mut_ptr(), 2) } == 1 {
            unsafe { FreeConsole() };
        }
    }

    pub fn stopping() -> bool {
        false
    }

    /// Pi is a batch file, pi.cmd, which cmd finds and runs.
    pub fn pi() -> Command {
        let mut command = Command::new("cmd");
        command.args(["/C", "pi"]);
        command
    }

    pub struct Group(HANDLE);

    pub fn spawn(command: &mut Command) -> std::io::Result<(Child, Group)> {
        let child = command.creation_flags(CREATE_NO_WINDOW).spawn()?;
        unsafe {
            let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            let size = std::mem::size_of_val(&limits) as u32;
            if job.is_null()
                || SetInformationJobObject(job, JobObjectExtendedLimitInformation, (&raw const limits).cast(), size) == 0
                || AssignProcessToJobObject(job, child.as_raw_handle() as HANDLE) == 0
            {
                return Err(std::io::Error::last_os_error());
            }
            Ok((child, Group(job)))
        }
    }

    impl Group {
        /// Ends every process of the job, Pi too if it still runs.
        pub fn end(self, pi: &mut Child) {
            unsafe { CloseHandle(self.0) };
            let _ = pi.wait();
        }
    }
}

/// A version's numbers, to compare.
fn version(version: &str) -> Vec<u64> {
    version.split('.').map(|n| n.parse().unwrap_or(0)).collect()
}

/// Downloads `release`'s archive for this machine, checks it against its digest, and puts its tines into `bin`.
fn fetch(release: &Release, bin: &Path) -> Result<()> {
    let dir = format!("tines-{}-{}", release.tag_name, target()?);
    let file = format!("{dir}.{}", if cfg!(windows) { "zip" } else { "tar.gz" });
    let asset = release.assets.iter().find(|a| a.name == file).with_context(|| format!("tines {} has no {file}", release.tag_name))?;
    let tmp = bin.with_file_name("tmp");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp)?;
    let archive = tmp.join(&file);
    run(Command::new("curl").arg("-fsSLo").arg(&archive).arg(&asset.browser_download_url))?;
    let digest = format!("sha256:{:x}", Sha256::digest(std::fs::read(&archive)?));
    ensure!(digest == asset.digest, "{file} does not match its release's digest");
    run(Command::new("tar").arg("-xf").arg(&archive).arg("-C").arg(&tmp))?;
    let (new, dest) = (tmp.join(&dir).join(exe("tines")), bin.join(exe("tines")));
    // Windows cannot overwrite a program that runs, but can move it aside.
    if cfg!(windows) && dest.exists() {
        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_millis();
        std::fs::rename(&dest, dest.with_extension(format!("{stamp}.old")))?;
    }
    std::fs::rename(new, dest)?;
    Ok(std::fs::remove_dir_all(&tmp)?)
}

/// Where tines keeps what it installs and runs.
fn home() -> Result<PathBuf> {
    Ok(std::env::home_dir().context("no home directory")?.join(".tines"))
}

/// This machine's target, as release archives name it.
fn target() -> Result<&'static str> {
    Ok(match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => "x86_64-unknown-linux-musl",
        ("linux", "aarch64") => "aarch64-unknown-linux-musl",
        ("macos", "x86_64") => "x86_64-apple-darwin",
        ("macos", "aarch64") => "aarch64-apple-darwin",
        ("windows", "x86_64") => "x86_64-pc-windows-msvc",
        (os, arch) => bail!("no release for {os} on {arch}"),
    })
}

fn exe(name: &str) -> String {
    format!("{name}{}", std::env::consts::EXE_SUFFIX)
}

/// A GitHub API answer.
fn get<T: serde::de::DeserializeOwned>(url: &str) -> Result<T> {
    let output = Command::new("curl").args(["-fsSL", "-H", "Accept: application/vnd.github+json", url]).output().context("cannot run curl")?;
    ensure!(output.status.success(), "curl {url}: {}", String::from_utf8_lossy(&output.stderr).trim());
    Ok(serde_json::from_slice(&output.stdout)?)
}

fn run(command: &mut Command) -> Result<()> {
    let output = command.output().with_context(|| format!("cannot run {}", command.get_program().display()))?;
    ensure!(output.status.success(), "{}: {}", command.get_program().display(), String::from_utf8_lossy(&output.stderr).trim());
    Ok(())
}
