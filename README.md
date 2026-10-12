# tines

Keeps one program running on every machine: a tine, always on in the background, started again whenever it ends.

Named for Vernor Vinge's Tines, packs of bodies that think as one mind over sound.

## Install

1. Download your platform's archive from the [latest release](https://github.com/spoj/tines/releases/latest) and run
   `tines update` from it: it installs tines into `~/.tines/bin`.
2. `~/.tines/bin/tines install <command>` gives this machine a tine, which runs `<command>`.

## Commands

- `tines update`: installs or updates tines in `~/.tines/bin`, checking the download against the digest its GitHub
  release lists. It needs `curl` and `tar`, which macOS, Linux and Windows 10 and later have.
- `tines install <command>`: saves `<command>`, its arguments joined by spaces, to `~/.tines/command`, and has the
  system run `tines keep` from `~/.tines/bin` from now on, and again after it restarts, with the PATH it runs with: a
  systemd user service on Linux, lingering so that it runs without a login; a launchd agent on macOS; on Windows, the
  user's Run key, which needs no administrator and takes the user's own PATH. Run again, it restarts the tine.
  `tines uninstall` undoes it.
- `tines keep`: keeps this machine's tine: the saved command, read anew at each start, which `sh`, or `cmd` on
  Windows, runs in `~/.tines/tine`. Its input stays open and empty, for programs that end with their input, such as Pi
  in RPC mode. When the tine ends, so does everything it started, and it starts again: after 2 s, or after longer waits,
  up to a minute, while it keeps ending within a minute of starting. Before each start, tines runs itself anew if
  `tines update` replaced it. It logs to `~/.tines/keep.log`, with what the tine writes to stderr.

## A Pi as the tine

    tines install pi --mode rpc --name tine -e git:github.com/spoj/pi-letmeknow

keeps a Pi that [pi-letmeknow](https://github.com/spoj/pi-letmeknow) puts in letmeknow as the session `tine`, so that
you reach it from anywhere; `-e` loads pi-letmeknow whether or not Pi has it installed, and refreshes it at each start.
Each start is a fresh context: what the tine should know, or remember, goes in `~/.tines/tine/AGENTS.md`, which Pi
reads. A tine updates itself: it runs `pi update --all` and `tines update`, then ends its Pi, which tines starts again.
