+++
title = "Stage: session"
summary = "Your desktop session starts: compositor, panels, background programs, until the desktop is usable."
related = ["session.start", "session.compositor", "session.shell", "session.ready"]
+++

After login, the display manager starts your **session**: the programs that run as
your user. On Wayland, the central one is the **compositor** (sway, Hyprland,
KWin, Mutter in GNOME Shell...), which owns the screen and the input devices. Around
it: a panel or bar, a notification daemon, the wallpaper, programs started
automatically.

kokuin considers the desktop **ready** when the session shell (panel, bar, desktop
shell) is running and, when recording live, when the CPU and the disks have calmed
down: what you perceive as "the computer finished booting".
