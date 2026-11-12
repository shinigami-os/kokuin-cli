+++
title = "getty: text login"
summary = "A program that opens a terminal, shows a login prompt and starts login."
related = ["stage.services", "graphics.display_manager"]
+++

Each text console (tty1, tty2..., what Ctrl+Alt+F1 to F6 show) needs a program to
show the `login:` prompt: **getty** (agetty, or BusyBox getty). It opens the
terminal, reads the user name and runs `login`, which checks the password and starts
the user's shell.

On a server, the getty on tty1 is the end of the boot. On a desktop, the display
manager often replaces it.
