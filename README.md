# kokuin-cli

Boot flow recorder and explainer: from the power button to a usable desktop.

`kokuin-cli` records what happens during boot (firmware, bootloader, kernel,
initramfs, init system, services, graphical session) and explains each step.
It is the backend and command-line front-end of **kokuin**. The graphical
`kokuin` app reads the JSON that `kokuin-cli` produces.

Built for [Kira Linux](https://github.com/shinigami-os), designed to work on any
Linux distribution.

Status: early development. Only the data format exists so far.

## Data format

The boot record format is described in [docs/format.md](docs/format.md), with a
JSON Schema in [schema/](schema/boot-record.schema.json) and an
[example](docs/examples/boot-record.example.json).

## Layout

| Path | Content |
|---|---|
| `crates/kokuin-model` | Rust types of the boot record; the JSON Schema is generated from them |
| `schema/` | Generated JSON Schema (do not edit by hand) |
| `docs/` | Format documentation and examples |

## Development

```sh
cargo test                                      # all tests
KOKUIN_UPDATE_SCHEMA=1 cargo test -p kokuin-model   # regenerate schema/ after a model change
cargo build --release --target x86_64-unknown-linux-musl   # static binaries
```

## License

GPL-2.0-only, see [LICENSE](LICENSE).
