+++
title = "Wayland compositor"
summary = "The program that owns the screen and the input devices, and combines application windows into one image."
related = ["stage.session", "graphics.drm", "session.shell"]

[[references]]
title = "Wayland"
url = "https://wayland.freedesktop.org/"
+++

With **Wayland**, the compositor is the display server: applications draw their
windows in buffers, and the compositor arranges them, adds decorations and effects,
and sends the final image to the screen through DRM/KMS. It also receives keyboard
and mouse input (through libinput) and gives it to the right window.

Examples: sway, Hyprland, KWin (KDE), Mutter (inside GNOME Shell), Weston, labwc,
niri. With X11, these jobs are split between the X server (Xorg) and a window manager.

The compositor is the heart of the graphical session: when it starts, you can see
something; when the shell around it is ready, you can use it.
