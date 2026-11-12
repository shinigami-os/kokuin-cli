# kokuin-cli

Boot flow recorder and explainer: from the power button to a usable desktop.

`kokuin-cli` records what happens during boot (firmware, bootloader, kernel,
initramfs, init system, services, graphical session) and explains each step.
It is the backend and command-line front-end of **kokuin**. The graphical
`kokuin` app reads the JSON that `kokuin-cli` produces.

Built for [Kira Linux](https://github.com/shinigami-os), designed to work on any
Linux distribution.

Status: early development. `kokuin-cli` rebuilds the current boot from what the
system remembers; the boot recorder (every process, live) comes next.

## Usage

```sh
sudo kokuin-cli status      # what can be read here, and why some data is missing
sudo kokuin-cli show        # summary of the current boot: chain, stages, milestones, slowest steps
sudo kokuin-cli timeline    # everything that happened, in order (--stage, --source, --kind, --grep)
kokuin-cli explain          # list the explanations; `kokuin-cli explain firmware.fpdt` shows one
sudo kokuin-cli capture -o boot.json   # save the record as JSON; `kokuin-cli show boot.json` reads it
```

Without root, kokuin still works but sees less: the kernel log, the TPM log and some
firmware data are readable by root only.

## Data format

The boot record format is described in [docs/format.md](docs/format.md), with a
JSON Schema in [schema/](schema/boot-record.schema.json) and an
[example](docs/examples/boot-record.example.json).

## Layout

| Path | Content |
|---|---|
| `crates/kokuin-model` | Rust types of the boot record; the JSON Schema is generated from them |
| `crates/kokuin-sources` | Readers of each data source (FPDT, TPM log, EFI variables, kernel log, /proc, init systems) |
| `crates/kokuin-kb` | Loads the knowledge base and its recognition rules |
| `crates/kokuin-analyze` | Merges the sources into a boot record: timeline, stages, milestones |
| `bins/kokuin-cli` | The command-line tool |
| `kb/articles/` | Explanations, one Markdown file per topic (`firmware/fpdt.md` is `firmware.fpdt`) |
| `kb/rules/` | Rules recognizing kernel log messages and programs |
| `schema/` | Generated JSON Schema (do not edit by hand) |
| `docs/` | Format documentation and examples |
| `tests/vm/` | QEMU test VMs (Alpine with OpenRC, Ubuntu with systemd), UEFI with a TPM |

Improving the explanations or teaching kokuin a new kernel message or program only
needs the files in `kb/`: no Rust.

## Development

```sh
cargo test                                      # all tests
KOKUIN_UPDATE_SCHEMA=1 cargo test -p kokuin-model   # regenerate schema/ after a model change
cargo build --release --target x86_64-unknown-linux-musl   # static binaries
```

To try kokuin-cli on other systems, see [the test VMs](tests/vm/README.md).

## License

GPL-2.0-only, see [LICENSE](LICENSE).
