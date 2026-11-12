+++
title = "runit stage 2: runsvdir"
summary = "runsvdir watches a directory of services and keeps each one running."
related = ["init.runit", "init.runit_stage1", "stage.services"]

[[references]]
title = "runsvdir manual"
url = "https://smarden.org/runit/runsvdir.8.html"
+++

`/etc/runit/2` usually ends with `exec runsvdir -P /some/directory`. That directory
holds the enabled services, often as symbolic links (`/var/service` on Void,
`/etc/sv` on Kira). `runsvdir` starts one `runsv` process per entry, and checks the
directory every few seconds: adding a link enables and starts a service, removing it
stops it.

kokuin marks the start of `runsvdir` as the beginning of the services stage, and
reads the directory `runsvdir` was given to find the services.
