# schemata

Schema converter with a rich intermediary language (XSD → .schemata → Protocol Buffers).

## The intermediary language

Every supported format converts through a common intermediary representation (IR)
rather than translating directly between formats. The IR's textual form is the
`.schemata` DSL: a compact, human-readable syntax that is inspectable, hand-editable,
and diffable, making it easy to review what a conversion produced or to author a
schema by hand before lowering it to a target format.

```schemata
schema justice.person @xml.namespace("http://example.org/person/1.0")

/// A human being involved in a case
record Person {
  /// Full legal name
  name: string @maxLength(100)
  ssn: SSN?
  aliases: string*
  status: PersonStatus
}

type SSN = string @pattern("\\d{3}-\\d{2}-\\d{4}")

enum PersonStatus {
  ACTIVE
  INACTIVE
}
```

A larger example covering the full language surface (cardinalities, choice
groups, constrained type aliases, pinned field numbers) lives in
[`examples/`](examples/).

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
# XSD → Protocol Buffers (default)
schemata convert --input schemas/ --output proto/

# XSD → .schemata (inspect or hand-edit the IR)
schemata convert --input schemas/ --output ir/ --to schemata

# .schemata → Protocol Buffers
schemata convert --input ir/ --output proto/
```

The input format is inferred from the file extensions or directory contents; pass
`--from xsd|schemata` to override it, and `--to proto|schemata` to select the output
format. Use `--profile niem|generic` to select the validation profile applied to the
input schemas.

Some conversions are lossy (for example, XSD constraints that have no equivalent in
Protocol Buffers). These emit warnings by default; pass `--deny-warnings` to turn them
into errors, which is useful in CI to catch unintentional data loss.

```sh
schemata --help
```

## License

MIT
