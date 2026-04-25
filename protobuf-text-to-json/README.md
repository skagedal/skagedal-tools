# protobuf-text-to-json

Converts Protocol Buffers text format to JSON.

**Note:** This tool is not well tested. Use with caution.

## Purpose

When debugging protobuf-encoded data, you might use `protoc --decode` to get a human-readable text representation. This tool converts that text format into JSON, which is often more convenient for processing with tools like `jq`.

## Building

```bash
cd protobuf-text-to-json
cargo build --release
```

Or install globally:

```bash
cargo install --path .
```

## Usage

The tool reads protobuf text format from stdin and writes JSON to stdout:

```bash
protobuf-text-to-json < input.textproto > output.json
```

Commonly used in a pipeline with `protoc`:

```bash
protoc --decode=MyMessage myproto.proto < message.bin | protobuf-text-to-json | jq .
```

## Features

- Parses standard protobuf text format as produced by `protoc --decode`
- Handles nested messages (both `{ }` and `< >` bracket styles)
- Converts repeated fields to JSON arrays automatically
- Supports all protobuf scalar types:
  - Strings (with escape sequences)
  - Integers (decimal and hex)
  - Floating point numbers (including `inf` and `nan`)
  - Booleans (`true`/`false`)
  - Enum values (converted to strings)
- Ignores comments (lines starting with `#`)
- Supports array literals `[1, 2, 3]`

## Examples

### Simple message

Input:
```
name: "John Doe"
id: 1234
active: true
```

Output:
```json
{
  "name": "John Doe",
  "id": 1234,
  "active": true
}
```

### Nested messages and repeated fields

Input:
```
person {
  name: "Alice"
  phones {
    number: "555-1234"
    type: MOBILE
  }
  phones {
    number: "555-4321"
    type: HOME
  }
}
```

Output:
```json
{
  "person": {
    "name": "Alice",
    "phones": [
      {
        "number": "555-1234",
        "type": "MOBILE"
      },
      {
        "number": "555-4321",
        "type": "HOME"
      }
    ]
  }
}
```

### Angle bracket syntax

Both `{ }` and `< >` bracket styles are supported:

```
config <
  setting: "value"
  nested <
    flag: true
  >
>
```

## Limitations

- Schema-less: Without the `.proto` schema, enum values are converted to strings rather than their numeric values
- Map fields appear as repeated entries rather than JSON objects with string keys
- Does not validate the structure against a schema
