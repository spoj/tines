# tines

Your agents on every machine, one pack, through [letmeknow](https://github.com/spoj/letmeknow): every interactive Pi
session is in letmeknow, and each machine can run a tine, a Pi always on in the background that you reach from anywhere.

Named for Vernor Vinge's Tines, packs of bodies that think as one mind over sound.

## Install

1. Download your platform's archive from the [latest release](https://github.com/spoj/tines/releases/latest) and run
   `tines update` from it: it installs tines and letmeknow, the latest 0.14.x, into `~/.tines/bin`.
2. `pi install git:github.com/spoj/tines` puts every interactive Pi session in letmeknow.
3. `~/.tines/bin/tines install` gives this machine a tine.

## The Pi extension

    pi install git:github.com/spoj/tines

It puts every interactive Pi session in letmeknow as the session `pi-<end of its Pi session id>`, which its status line
shows. What letmeknow prints for the session reaches the agent as a message that starts a turn, and the session is
removed when the Pi session ends in no group. `--letmeknow <handle>` puts a session of any mode in letmeknow as that
session, kept when it ends. It runs `~/.tines/bin/letmeknow`, so run `tines update` first.

## Commands

- `tines update`: installs or updates letmeknow and tines in `~/.tines/bin`, checking each download against the digest
  its GitHub release lists. It needs `curl` and `tar`, which macOS, Linux and Windows 10 and later have.
- `tines install`: has the system run `tines keep` from `~/.tines/bin` from now on, and again after it restarts, with
  the PATH it runs with: a systemd user service on Linux, lingering so that it runs without a login; a launchd agent on
  macOS; a task at log on on Windows, which takes the user's own PATH. `tines uninstall` undoes it.
- `tines keep`: keeps this machine's tine, `pi --mode rpc --letmeknow tine` working in `~/.tines/tine`, which needs the
  extension. When Pi ends, so does everything it started, and Pi starts again with a fresh context: after 2 s, or after
  longer waits, up to a minute, while it keeps ending within a minute of starting. Before each start, tines runs itself
  anew if `tines update` replaced it. It logs to `~/.tines/keep.log`, with what Pi writes to stderr.

A tine updates itself: it runs `pi update` and `tines update`, then ends its Pi, which the keeper starts again.
