# kokuin-cli

Boot flow recorder and explainer: from the power button to a usable desktop.

`kokuin-cli` records what happens during boot (firmware, bootloader, kernel,
initramfs, init system, services, graphical session) and explains each step.
It is the backend and command-line front-end of **kokuin**. The graphical
`kokuin` app reads the JSON that `kokuin-cli` produces.

Built for [Kira Linux](https://github.com/shinigami-os), designed to work on any
Linux distribution.

Status: early development.

## License

GPL-2.0-only, see [LICENSE](LICENSE).
