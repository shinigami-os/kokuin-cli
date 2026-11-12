+++
title = "runit"
summary = "A small, simple init system and service supervisor, used by Kira, Void and Artix."
related = ["init.runit_stage1", "init.runit_stage2", "init.pid1", "stage.services"]

[[references]]
title = "runit"
url = "https://smarden.org/runit/"
+++

**runit** does three things, in three stages, each one a plain script you can read:

1. `/etc/runit/1`: one-time system setup (mounts, udev, hostname...). runit waits
   for it to finish.
2. `/etc/runit/2`: runs for the whole life of the system. It usually executes
   `runsvdir`, which supervises the services.
3. `/etc/runit/3`: shutdown, when stage 2 ends.

A **service** is a directory with a `run` script that starts the program in the
foreground. `runsvdir` starts one `runsv` per service directory; `runsv` runs the
service and restarts it when it exits. `sv status <service>` shows its state, `sv
up/down <service>` controls it. A `down` file in the directory keeps a service down
at boot.

There are no dependencies and no ordering: everything starts in parallel. A service
that needs another one checks for it in its `run` script and exits if it is not
ready, and `runsv` tries again a second later.

runit does not record when a service became *ready*, only when its process started.
