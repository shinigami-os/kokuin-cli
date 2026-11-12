+++
title = "Recording processes live"
summary = "The kernel can notify a program of every process creation, program execution and exit."
related = ["init.procfs", "init.pid1"]
+++

The kernel's **process events connector** (`CONFIG_PROC_EVENTS`) sends a message
through a netlink socket each time a process is created (`fork`), runs a new
program (`exec`) or exits, with a precise timestamp.

kokuin's recorder starts first, just before the init system, and listens to these
events. It therefore sees every process of the boot, even those that lived only a
few milliseconds, with exact start and end times. This is how the boot of an init
system that keeps no timing records (like runit) can be measured precisely.
