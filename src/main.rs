//! tines: your agents on every machine, one pack, through letmeknow. See README.md.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail, ensure};
use serde::Deserialize;
use sha2::{Digest, Sha256};

/// The letmeknow minor version that tines installs, and that its extension's sessions speak.
const LETMEKNOW: &str = "0.14";

/// What letmeknow's release archives hold.
const LETMEKNOW_EXES: [&str; 4] = ["letmeknow", "letmeknow-kind-doc", "letmeknow-kind-git", "git-remote-lmk"];

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    draft: bool,
    prerelease: bool,
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
        _ => bail!("usage: tines update"),
    }
}

/// Installs or updates letmeknow's latest release of its minor version, then tines itself, into `~/.tines/bin`.
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

    let releases: Vec<Release> = get("https://api.github.com/repos/spoj/letmeknow/releases?per_page=100")?;
    let release = newest(releases, LETMEKNOW).with_context(|| format!("letmeknow has no {LETMEKNOW} release"))?;
    let installed = Command::new(bin.join(exe("letmeknow"))).arg("--version").output().ok();
    let installed = installed.map(|o| String::from_utf8_lossy(&o.stdout).trim().trim_start_matches("letmeknow ").to_owned());
    let latest = &release.tag_name[1..];
    if installed.as_deref() == Some(latest) {
        println!("letmeknow {latest} is current");
    } else {
        install(&release, "letmeknow", &LETMEKNOW_EXES, &bin)?;
        println!("letmeknow {} → {latest}", installed.as_deref().unwrap_or("none"));
    }

    let release: Release = get("https://api.github.com/repos/spoj/tines/releases/latest")?;
    let running = env!("CARGO_PKG_VERSION");
    let latest = &release.tag_name[1..];
    if version(latest) > version(running) || !bin.join(exe("tines")).exists() {
        install(&release, "tines", &["tines"], &bin)?;
        println!("tines {running} → {latest}");
    } else {
        println!("tines {running} is current");
    }
    Ok(())
}

/// The newest release, neither draft nor prerelease, of the minor version `minor`.
fn newest(releases: Vec<Release>, minor: &str) -> Option<Release> {
    let prefix = format!("v{minor}.");
    releases.into_iter().filter(|r| !r.draft && !r.prerelease && r.tag_name.starts_with(&prefix)).max_by_key(|r| version(&r.tag_name[1..]))
}

/// A version's numbers, to compare.
fn version(version: &str) -> Vec<u64> {
    version.split('.').map(|n| n.parse().unwrap_or(0)).collect()
}

/// Downloads `release`'s archive of `name` for this machine, checks it against its digest, and puts `exes` from it into
/// `bin`.
fn install(release: &Release, name: &str, exes: &[&str], bin: &Path) -> Result<()> {
    let dir = format!("{name}-{}-{}", release.tag_name, target()?);
    let file = format!("{dir}.{}", if cfg!(windows) { "zip" } else { "tar.gz" });
    let asset = release.assets.iter().find(|a| a.name == file).with_context(|| format!("{name} {} has no {file}", release.tag_name))?;
    let tmp = bin.with_file_name("tmp");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp)?;
    let archive = tmp.join(&file);
    run(Command::new("curl").arg("-fsSLo").arg(&archive).arg(&asset.browser_download_url))?;
    let digest = format!("sha256:{:x}", Sha256::digest(std::fs::read(&archive)?));
    ensure!(digest == asset.digest, "{file} does not match its release's digest");
    run(Command::new("tar").arg("-xf").arg(&archive).arg("-C").arg(&tmp))?;
    for name in exes {
        let (new, dest) = (tmp.join(&dir).join(exe(name)), bin.join(exe(name)));
        // Windows cannot overwrite a program that runs, but can move it aside.
        if cfg!(windows) && dest.exists() {
            let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_millis();
            std::fs::rename(&dest, dest.with_extension(format!("{stamp}.old")))?;
        }
        std::fs::rename(new, dest)?;
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_newest_release_of_a_minor_version_counts_its_patch_as_a_number() {
        let release = |tag: &str, prerelease| Release { tag_name: tag.into(), draft: false, prerelease, assets: Vec::new() };
        let releases = vec![release("v0.14.9", false), release("v0.15.0", false), release("v0.14.10", false), release("v0.14.11", true)];
        assert_eq!(newest(releases, "0.14").map(|r| r.tag_name), Some("v0.14.10".into()));
    }
}
