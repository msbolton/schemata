# schemata

XSD to Protocol Buffers converter for NIEM-based schemas.

## Installation

### Pre-built binaries

Download the latest release for your platform from
[GitHub Releases](https://github.com/msbolton/schemata/releases/latest).

| Platform | Target |
|----------|--------|
| Linux x86_64 | `schemata-*-x86_64-unknown-linux-gnu.tar.gz` |
| Linux aarch64 | `schemata-*-aarch64-unknown-linux-gnu.tar.gz` |
| macOS x86_64 | `schemata-*-x86_64-apple-darwin.tar.gz` |
| macOS Apple Silicon | `schemata-*-aarch64-apple-darwin.tar.gz` |
| Windows x86_64 | `schemata-*-x86_64-pc-windows-msvc.zip` |

Extract and place the binary in your `PATH`.

### From source

```sh
cargo install --git https://github.com/msbolton/schemata
```

## Usage

```sh
schemata --help
```

## License

MIT
