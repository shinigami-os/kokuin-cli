+++
title = "DRM/KMS graphics driver"
summary = "The kernel driver of the graphics card: it sets the screen mode and lets programs draw."
related = ["stage.graphical", "graphics.fbcon", "session.compositor"]

[[references]]
title = "Linux GPU driver documentation"
url = "https://docs.kernel.org/gpu/index.html"
+++

The **Direct Rendering Manager** (DRM) is the kernel subsystem for graphics cards.
**KMS** (Kernel Mode Setting) is its part that sets the screen resolution and shows
images on screens. Each card has its driver: `i915` and `xe` (Intel), `amdgpu`,
`nouveau` (NVIDIA, or NVIDIA's own `nvidia-drm`), `virtio_gpu` (virtual machines)...

`[drm] Initialized amdgpu ...` means the driver is ready: from then on, a compositor
can use the card through `/dev/dri/card0`. Before that, only the firmware's simple
framebuffer (`simpledrm`, `efifb`) shows something.

Graphics drivers often need firmware files loaded from disk, which can delay them.
