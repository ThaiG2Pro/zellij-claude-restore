#!/usr/bin/env python3
"""Claude Code SessionStart hook for zellij-claude-restore.

Records the current Claude session UUID in a cwd-keyed marker file that the
zellij-claude-restore WASM plugin reads when it enriches a saved layout.

Why this exact path:
    The plugin runs inside a WASI sandbox where guest ``/tmp`` is mapped to the
    host's ``ZELLIJ_TMP_DIR`` (``/tmp/zellij-<uid>``), NOT the real ``/tmp``.
    The plugin reads ``/tmp/claude-sessions/<encoded-cwd>.session`` inside the
    sandbox, so on the host the marker must live at
    ``/tmp/zellij-<uid>/claude-sessions/<encoded-cwd>.session``.

Why keyed by cwd (not PID, as the original HANDOFF design proposed):
    The dumped layout KDL carries each pane's cwd but no pane PID, so the plugin
    can only match a marker by cwd. ``<encoded-cwd>`` is the absolute cwd with
    ``/`` replaced by ``-`` (the same scheme Claude uses for
    ``~/.claude/projects/<encoded-cwd>/``).
    Limitation: two Claude panes sharing one cwd collide on a single marker.

Input: Claude Code delivers a JSON object on stdin, including ``session_id`` and
``cwd``. We never trust ``$CLAUDE_SESSION_ID`` from the environment because it is
empty in ordinary tool-call shells.

This hook always exits 0 and never prints to stdout, so it can't disrupt Claude.
"""

import json
import os
import sys


def main() -> None:
    try:
        data = json.load(sys.stdin)
    except Exception:
        return  # malformed/empty stdin — nothing we can do, stay silent

    session_id = (data.get("session_id") or "").strip()
    cwd = (data.get("cwd") or os.getcwd()).rstrip("/")
    if not session_id or not cwd:
        return

    marker_dir = f"/tmp/zellij-{os.getuid()}/claude-sessions"
    encoded = cwd.replace("/", "-")
    try:
        os.makedirs(marker_dir, exist_ok=True)
        # Write atomically so a half-written marker is never read by the plugin.
        tmp_path = os.path.join(marker_dir, f".{encoded}.session.tmp")
        final_path = os.path.join(marker_dir, f"{encoded}.session")
        with open(tmp_path, "w") as fh:
            fh.write(session_id)
        os.replace(tmp_path, final_path)
    except OSError:
        return


if __name__ == "__main__":
    main()
    sys.exit(0)
