+++
title = "The kernel frees its initialization code"
summary = "Code only needed during boot is thrown away to give the memory back."
related = ["stage.kernel", "kernel.run_init"]
+++

A lot of kernel code only runs once, during boot: the initcalls and their helpers are
marked `__init`. Right before starting the first process, the kernel frees that
memory (`Freeing unused kernel image (initmem) memory: 4096K`) and makes its own
code read-only.

This is one of the last messages of the kernel stage.
