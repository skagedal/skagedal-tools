# jsonl-stream-processor

A lightweight tool that processes JSONL (JSON Lines) streams, ensuring all output is valid JSON.

## Purpose

When processing log streams or command output, you often get a mix of valid JSON and plain text. This tool reads line-by-line from stdin and:
- Outputs valid JSON lines unchanged
- Wraps non-JSON lines in a JSON envelope: `{"message":"..."}`

This ensures your entire output stream is valid JSONL, making it easier to pipe to other JSON processing tools.

## Building

```bash
cargo build --release
```

The binary will be at `target/release/jsonl-stream-processor`.

## Usage

```bash
# Basic usage
some-command | jsonl-stream-processor

# Example with mixed output
echo '{"status":"ok","count":42}' | jsonl-stream-processor
# Output: {"status":"ok","count":42}

echo 'Plain text message' | jsonl-stream-processor
# Output: {"message":"Plain text message"}

# Real-world example: process kubectl output
kubectl get pods --watch -o json | jsonl-stream-processor | jq .

# Process log files with mixed content
cat application.log | jsonl-stream-processor | jq -r '.message // .'
```

## Examples

### Mixed JSON and text input

```bash
$ cat <<EOF | jsonl-stream-processor
{"level":"info","msg":"Server started"}
Error: connection timeout
{"level":"error","msg":"Failed to connect"}
Warning: retrying...
EOF
```

Output:
```json
{"level":"info","msg":"Server started"}
{"message":"Error: connection timeout"}
{"level":"error","msg":"Failed to connect"}
{"message":"Warning: retrying..."}
```

### Special characters and escaping

The tool properly escapes special characters in non-JSON lines:

```bash
$ echo 'Message with "quotes" and \backslashes' | jsonl-stream-processor
{"message":"Message with \"quotes\" and \\backslashes"}
```

## Alternatives

Before using this tool, consider these alternatives:

### Existing Tools

1. **jq** - The swiss-army knife of JSON processing
   ```bash
   # Try to parse as JSON, otherwise wrap in object
   some-command | jq -R 'fromjson? // {message: .}'
   ```
   This is probably the most common solution and doesn't require any additional tools.

2. **gron** - Makes JSON greppable by flattening it
   ```bash
   # Not the same use case, but useful for exploring JSON
   cat data.json | gron | grep pattern | gron --ungron
   ```
   Project: https://github.com/tomnomnom/gron

3. **jl** (JSON Logs) - Tool for working with line-delimited JSON
   ```bash
   # Has filtering and formatting capabilities
   cat logs.jsonl | jl 'select(.level == "error")'
   ```
   Project: https://github.com/koenbollen/jl

4. **jsonlines-cli** - Python-based tool for JSONL processing
   ```bash
   pip install jsonlines-cli
   cat data.jsonl | jsonlines validate
   ```

5. **Miller (mlr)** - Like awk/sed/cut for structured data including JSON
   ```bash
   # Can convert between formats and handle mixed input
   some-command | mlr --json cat
   ```
   Project: https://miller.readthedocs.io/

### Shell-based Solutions

If you don't want to install additional tools:

```bash
# Using jq (most common, requires jq)
some-command | while IFS= read -r line; do
  echo "$line" | jq -e . >/dev/null 2>&1 && echo "$line" || \
    jq -n --arg msg "$line" '{message: $msg}'
done

# Using Python (available almost everywhere)
some-command | python3 -c '
import sys, json
for line in sys.stdin:
    line = line.rstrip("\n\r")
    try:
        json.loads(line)
        print(line)
    except:
        print(json.dumps({"message": line}))
'

# Using awk and jq
some-command | awk '{
  cmd = "echo \047" $0 "\047 | jq -e . >/dev/null 2>&1"
  if (system(cmd) == 0) {
    print
  } else {
    cmd = "jq -n --arg msg \047" $0 "\047 \047{message: $msg}\047"
    system(cmd)
  }
}'
```

### When to use this tool

Use `jsonl-stream-processor` when:
- You want a fast, standalone binary with no dependencies
- You process high-volume streams where performance matters
- You prefer a simple, single-purpose tool
- You don't want to rely on jq or Python being installed

Use alternatives when:
- jq is already available (use `jq -R 'fromjson? // {message: .}'`)
- You need additional JSON processing capabilities beyond normalization
- You're already using a scripting language in your pipeline
- You want to avoid installing yet another tool
