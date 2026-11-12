+++
title = "Stage: services"
summary = "The init system starts the long-running programs (daemons) that provide the system's features."
related = ["init.runit_stage2", "init.systemd", "init.openrc", "stage.graphical"]
+++

Once the base system is ready, the init system starts **services**: programs that
run in the background for the whole life of the system. Logging (syslog), network
configuration, SSH server, time synchronization, login prompts...

How it is done shows the philosophy of each init system:

- **runit** starts all service directories at once, in parallel, and restarts them if
  they die. There are no dependencies: a service that needs another one checks for it
  and fails (and is restarted) until it is there.
- **OpenRC** reads dependencies declared in each service script (`need`, `after`)
  and starts services in that order, in parallel when possible.
- **systemd** builds a dependency graph of units and starts as much as possible in
  parallel, also using socket and D-Bus activation.

This stage has no clear end: some services take long to become ready, and some are
started later on demand.
