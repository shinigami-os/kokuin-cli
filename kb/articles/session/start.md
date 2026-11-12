+++
title = "Session start"
summary = "The first program running as you: the login is done."
related = ["stage.session", "graphics.display_manager", "session.compositor"]
+++

After a successful login, the display manager (or `login`) starts the session
programs **as your user**. kokuin detects this moment as the first process of a
normal user (user id 1000 or above) during the boot.

With automatic login, this happens without waiting for you. Otherwise, the time you
spend typing your password is part of the measured boot: compare boots carefully.
