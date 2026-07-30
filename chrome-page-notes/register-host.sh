#!/usr/bin/env bash
#
# Builds the native messaging host and registers it with Chrome so the
# chrome-page-notes extension can talk to it. Safe to re-run any time,
# e.g. after moving the repo or updating the host.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKSPACE_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

# Matches the extension's manifest.json "key" field, which pins the
# extension to this ID regardless of which machine/path it's loaded
# unpacked from.
EXTENSION_ID="jbgofjilflakfjbenbgpppajapiffphn"
HOST_NAME="com.skagedal.chrome_page_notes_host"

echo "Building $HOST_NAME..."
(cd "$WORKSPACE_DIR" && cargo build --release -p chrome-page-notes-host)

BINARY="$WORKSPACE_DIR/target/release/chrome-page-notes-host"
MANIFEST_DIR="$HOME/Library/Application Support/Google/Chrome/NativeMessagingHosts"
MANIFEST_PATH="$MANIFEST_DIR/$HOST_NAME.json"

mkdir -p "$MANIFEST_DIR"
cat > "$MANIFEST_PATH" <<EOF
{
  "name": "$HOST_NAME",
  "description": "chrome-page-notes native messaging host",
  "path": "$BINARY",
  "type": "stdio",
  "allowed_origins": ["chrome-extension://$EXTENSION_ID/"]
}
EOF

echo "Registered $MANIFEST_PATH"
echo "  -> $BINARY"
