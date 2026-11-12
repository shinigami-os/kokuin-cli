+++
title = "Stage: graphical"
summary = "The graphics stack starts and a login screen (display manager) appears."
related = ["graphics.drm", "graphics.display_manager", "session.compositor", "stage.session"]
+++

Graphics on Linux are layered:

1. In the kernel, a **DRM/KMS driver** (i915, amdgpu, nouveau, virtio_gpu...)
   drives the graphics card and sets the screen mode.
2. In userspace, a **display server** draws on the screen through that driver: a
   Wayland compositor, or the X server (Xorg).
3. A **display manager** (greetd, GDM, SDDM, LightDM, ly...) shows a login screen,
   using its own small compositor, then starts your session.

This stage starts when the display manager starts, and ends when a user session
begins. Systems with automatic login, or that start the desktop from a text login,
can skip the display manager.
