# tines

Keeps a Pi running on every machine: a tine, Pi always on in the background, which
[pi-letmeknow](https://github.com/spoj/pi-letmeknow) puts in letmeknow as the session `tine`, so that you reach it from
anywhere.

Named for Vernor Vinge's Tines, packs of bodies that think as one mind over sound.

## Install

1. Download your platform's archive from the [latest release](https://github.com/spoj/tines/releases/latest) and run
   `tines update` from it: it installs tines into `~/.tines/bin`.
2. `~/.tines/bin/tines install` gives this machine a tine.

## Commands

- `tines update`: installs or updates tines in `~/.tines/bin`, checking the download against the digest its GitHub
  release lists. It needs `curl` and `tar`, which macOS, Linux and Windows 10 and later have.
- `tines install`: has the system run `tines keep` from `~/.tines/bin` from now on, and again after it restarts, with
  the PATH it runs with: a systemd user service on Linux, lingering so that it runs without a login; a launchd agent on
  macOS; on Windows, the user's Run key, which needs no administrator and takes the user's own PATH. `tines uninstall` undoes it.
- `tines keep`: keeps this machine's tine, `pi --mode rpc --name tine -e git:github.com/spoj/pi-letmeknow` working in
  `~/.tines/tine`, which loads pi-letmeknow whether or not Pi has it installed, and refreshes it at each start. When Pi ends, so does
  everything it started, and Pi starts again with a fresh context: after 2 s, or after longer waits, up to a minute,
  while it keeps ending within a minute of starting. Before each start, tines runs itself anew if `tines update`
  replaced it. It logs to `~/.tines/keep.log`, with what Pi writes to stderr.

A tine updates itself: it runs `pi update` and `tines update`, then ends its Pi, which the keeper starts again.
