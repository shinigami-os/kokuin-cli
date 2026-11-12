+++
title = "Framebuffer console"
summary = "The kernel's text console drawn on a graphical screen."
related = ["graphics.drm", "stage.kernel"]
+++

The text console you see during boot (and on Ctrl+Alt+F2) is drawn by the kernel
itself: **fbcon** renders text into the framebuffer of a graphics driver.

At first it uses the simple framebuffer left by the firmware; when the real DRM driver
loads, the console moves to it (`fbcon: amdgpudrmfb (fb0) is primary device`). The
screen may flicker or change resolution at that moment.
