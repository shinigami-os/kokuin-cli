+++
title = "Display manager (login screen)"
summary = "The program that shows the graphical login screen and starts your session."
related = ["stage.graphical", "session.start", "init.getty"]
+++

A **display manager** is a service that shows a graphical login screen ("greeter"),
checks your password (through PAM) and starts your session as your user.

Examples: GDM (GNOME), SDDM (KDE), LightDM, ly (in the text console), and
**greetd**, a minimal one that delegates the greeter to any program. Kira uses greetd
with `cage` (a compositor that shows a single window) and a Quickshell greeter.

The greeter runs as a dedicated system user, not as you. kokuin uses this to tell
the login screen (graphical stage) apart from your session.
