#!/usr/bin/env python3
"""
Notchai hook script for Claude Code.

Bridges Claude Code hook events to the Notchai app via Unix domain socket.
For PermissionRequest events, blocks waiting for the user's approval decision.

Uses only Python stdlib. Fails open on any error (exits 0 silently)
so Claude Code is never blocked if the app isn't running.
"""

import json
import os
import socket
import sys
import time

SOCKET_PATH = "/tmp/notchai.sock"
MAX_TOOL_INPUT_LEN = 500
PERMISSION_TIMEOUT = 300  # seconds


def truncate(value, max_len):
    """Truncate a string value to max_len characters."""
    if value is None:
        return None
    s = str(value)
    if len(s) <= max_len:
        return s
    return s[:max_len - 3] + "..."


def truncate_tool_input_fields(tool_input_raw, max_field_len=300):
    """Truncate individual string fields in tool_input while keeping valid JSON.

    Instead of truncating the serialized JSON string (which breaks parsing),
    this truncates each string value individually so the JSON structure stays valid.
    """
    if not isinstance(tool_input_raw, dict):
        return truncate(str(tool_input_raw), max_field_len)
    result = {}
    for key, value in tool_input_raw.items():
        if isinstance(value, str) and len(value) > max_field_len:
            result[key] = value[:max_field_len - 3] + "..."
        else:
            result[key] = value
    return json.dumps(result)


def build_hook_message(hook_input):
    """Build a HookMessage from the Claude Code hook input JSON."""
    event_type = hook_input.get("hook_event_name", "")
    tool_name = hook_input.get("tool_name", "")

    # tool_input may be a dict; serialize it for transport
    tool_input_raw = hook_input.get("tool_input")
    if tool_input_raw is not None:
        # Don't truncate AskUserQuestion — the frontend needs the full input
        is_ask_question = (
            event_type == "PermissionRequest" and tool_name == "AskUserQuestion"
        )
        if is_ask_question:
            if isinstance(tool_input_raw, dict):
                tool_input_str = json.dumps(tool_input_raw)
            else:
                tool_input_str = str(tool_input_raw)
        else:
            # Truncate individual fields to keep JSON valid and parseable
            tool_input_str = truncate_tool_input_fields(tool_input_raw)
    else:
        tool_input_str = None

    msg = {
        "event_type": event_type,
        "session_id": hook_input.get("session_id"),
        "cwd": hook_input.get("cwd"),
        "tool_name": hook_input.get("tool_name"),
        "tool_input": tool_input_str,
        "tool_use_id": hook_input.get("tool_use_id"),
        "agent": "claude",
        "timestamp": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
    }

    # Forward permission_suggestions so the UI knows if "Always Allow" is available
    permission_suggestions = hook_input.get("permission_suggestions")
    if permission_suggestions:
        msg["permission_suggestions"] = json.dumps(permission_suggestions)

    return msg


def send_to_socket(message, wait_for_response=False):
    """Send a JSON message to the Notchai Unix socket.

    If wait_for_response is True, blocks until a response is received
    or the timeout is reached.

    Returns the parsed response dict, or None.
    """
    sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    try:
        sock.connect(SOCKET_PATH)
    except (socket.error, OSError):
        # App not running or socket unavailable - fail open
        sock.close()
        return None

    try:
        payload = json.dumps(message) + "\n"
        sock.sendall(payload.encode("utf-8"))

        if not wait_for_response:
            return None

        # Wait for the permission decision
        sock.settimeout(PERMISSION_TIMEOUT)
        buf = b""
        while True:
            chunk = sock.recv(4096)
            if not chunk:
                break
            buf += chunk
            # Response is a single JSON object terminated by newline
            if b"\n" in buf:
                break

        if buf:
            return json.loads(buf.decode("utf-8").strip())
        return None
    except (socket.timeout, socket.error, OSError):
        # Timeout or error - fail open
        return None
    except (json.JSONDecodeError, ValueError):
        # Malformed response - fail open
        return None
    finally:
        try:
            sock.close()
        except OSError:
            pass


def main():
    # Read hook input from stdin
    try:
        raw = sys.stdin.read()
        if not raw.strip():
            sys.exit(0)
        hook_input = json.loads(raw)
    except (json.JSONDecodeError, ValueError, IOError):
        # Can't parse input - fail open
        sys.exit(0)

    event_type = hook_input.get("hook_event_name", "")
    message = build_hook_message(hook_input)
    is_permission_request = event_type == "PermissionRequest"

    response = send_to_socket(message, wait_for_response=is_permission_request)

    if is_permission_request and response:
        decision = response.get("decision", "allow")
        reason = response.get("reason")
        updated_input_str = response.get("updated_input")
        updated_permissions_str = response.get("updated_permissions")

        if decision == "deny":
            output = {
                "hookSpecificOutput": {
                    "hookEventName": "PermissionRequest",
                    "decision": {
                        "behavior": "deny",
                        "message": reason or "Denied by user in Notchai",
                    },
                }
            }
        else:
            decision_obj = {"behavior": "allow"}
            # For AskUserQuestion: include updatedInput with user's answers
            if updated_input_str:
                try:
                    decision_obj["updatedInput"] = json.loads(updated_input_str)
                except (json.JSONDecodeError, ValueError):
                    pass
            # For "Always Allow": include updatedPermissions rules
            if updated_permissions_str:
                try:
                    decision_obj["updatedPermissions"] = json.loads(updated_permissions_str)
                except (json.JSONDecodeError, ValueError):
                    pass
            output = {
                "hookSpecificOutput": {
                    "hookEventName": "PermissionRequest",
                    "decision": decision_obj,
                }
            }

        sys.stdout.write(json.dumps(output))
        sys.stdout.write("\n")
        sys.stdout.flush()

    # Always exit 0 - fail open
    sys.exit(0)


if __name__ == "__main__":
    main()
