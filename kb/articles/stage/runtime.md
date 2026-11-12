+++
title = "Stage: runtime"
summary = "After the desktop is ready: the boot is over, but the system keeps doing things."
related = ["stage.session", "session.ready"]
+++

Everything after the desktop became ready is no longer part of the boot. kokuin
still records it for a few seconds (the "settle time"): programs started
automatically, late devices, background tasks that slow down the first minute of
use. The longer the system stays busy after the desktop appears, the less "ready" it
really is.
