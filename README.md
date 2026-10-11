# tines

Your agents on every machine, one pack, through [letmeknow](https://github.com/spoj/letmeknow): every interactive Pi
session is in letmeknow, and each machine can run a tine, a Pi always on in the background that you reach from anywhere.

Named for Vernor Vinge's Tines, packs of bodies that think as one mind over sound.

## Install

Download your platform's archive from the [latest release](https://github.com/spoj/tines/releases/latest) and run
`tines update` from it. It installs tines and letmeknow, the latest 0.14.x, into `~/.tines/bin`; run it again to update
them.

## The Pi extension

    pi install git:github.com/spoj/tines

It puts every interactive Pi session in letmeknow as the session `pi-<end of its Pi session id>`, which its status line
shows. What letmeknow prints for the session reaches the agent as a message that starts a turn, and the session is
removed when the Pi session ends in no group. `--letmeknow <handle>` puts a session of any mode in letmeknow as that
session, kept when it ends. It runs `~/.tines/bin/letmeknow`, so run `tines update` first.

## Commands

- `tines update`: installs or updates letmeknow and tines in `~/.tines/bin`, checking each download against the digest
  its GitHub release lists. It needs `curl` and `tar`, which macOS, Linux and Windows 10 and later have.
