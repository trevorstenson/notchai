#!/usr/bin/env bash
# Notchai Codex notify relay script.
#
# Invoked by Codex's notify command on turn completion.
# Sends a JSON payload to the Notchai Unix socket so the app
# can show real-time Codex session status.
#
# Fail-open: exits 0 on any error so Codex is never blocked.

set -e
trap 'exit 0' ERR

SOCKET_PATH="/tmp/notchai.sock"
SESSION_ID="${CODEX_SESSION_ID:-unknown}"
CWD="$(pwd)"
TIMESTAMP="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"

PAYLOAD=$(cat <<EOF
{"event_type":"task_complete","session_id":"${SESSION_ID}","cwd":"${CWD}","agent":"codex","source":"codex","timestamp":"${TIMESTAMP}"}
EOF
)

# Try socat first, fall back to python3
if command -v socat >/dev/null 2>&1; then
    printf '%s\n' "$PAYLOAD" | socat - UNIX-CONNECT:"$SOCKET_PATH" 2>/dev/null || true
elif command -v python3 >/dev/null 2>&1; then
    python3 -c "
import socket, sys
s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
try:
    s.connect('${SOCKET_PATH}')
    s.sendall((sys.argv[1] + '\n').encode())
except Exception:
    pass
finally:
    s.close()
" "$PAYLOAD" 2>/dev/null || true
fi

exit 0
