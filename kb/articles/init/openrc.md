+++
title = "OpenRC"
summary = "A dependency-based init system made of shell scripts, used by Alpine, Gentoo and Artix."
related = ["init.pid1", "stage.init", "stage.services"]

[[references]]
title = "OpenRC"
url = "https://github.com/OpenRC/openrc"
+++

**OpenRC** starts services through scripts in `/etc/init.d/`, which declare their
dependencies (`need net`, `after firewall`). Services are grouped in **runlevels**
(`/etc/runlevels/`): `sysinit` (devices, virtual filesystems), `boot` (local
filesystems, hostname, clock), then `default` (your services).

PID 1 can be OpenRC's own `openrc-init`, or another init (BusyBox `init`, sysvinit)
that calls `openrc`. Started services are tracked in `/run/openrc/started/`;
`rc-status` shows them.
