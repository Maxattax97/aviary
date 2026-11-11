#!/usr/bin/env python3
"""Verify selected previews in wide and narrow terminal panes."""
import subprocess
import tempfile
import time
from pathlib import Path
from support import BINARY, Service


def main():
    with tempfile.TemporaryDirectory(prefix="aviary-preview-") as directory, Service() as service:
        socket = str(Path(directory) / "tmux.sock")
        def tmux(*args, check=True):
            return subprocess.run(["tmux", "-f", "/dev/null", "-S", socket, *args], text=True, capture_output=True, timeout=5, check=check)
        def wait_for(marker):
            deadline = time.monotonic() + 8
            while time.monotonic() < deadline:
                result = tmux("capture-pane", "-p", "-t", "aviary:", check=False)
                if result.returncode == 0 and marker in result.stdout:
                    return result.stdout
                time.sleep(0.05)
            raise AssertionError(f"missing {marker!r}: {result.stdout}")
        ids = [service.call({"op": "submit", "request": {"identity": f"preview-{i}", "agent": f"agent-{i}", "question": f"Preview question {i}", "context": f"Full context {i}", "options": []}})["id"] for i in (1, 2)]
        service.call({"op": "create_channel", "channel": {"name": "preview", "members": ["operator", "agent"]}})
        for i in (1, 2):
            service.call({"op": "send_message", "message": {"identity": f"preview-message-{i}", "channel": "preview", "sender": "agent", "target": {"kind": "participant", "name": "operator"}, "body": f"Preview message {i}"}})
            service.call({"op": "submit_notice", "notice": {"identity": f"preview-notice-{i}", "agent": "agent", "work": f"Preview work {i}", "result": f"Preview result {i}"}})
        try:
            tmux("new-session", "-d", "-s", "aviary", "-x", "100", "-y", "20", str(BINARY), "tui", service.address)
            wait_for("Question: Preview question 1")
            tmux("send-keys", "-t", "aviary:", "Tab", "j")
            wait_for("Question: Preview question 2")
            tmux("resize-window", "-t", "aviary:", "-x", "40", "-y", "12")
            wait_for("Question: Preview question 2")
            tmux("send-keys", "-t", "aviary:", "k")
            wait_for("Question: Preview question 1")
            tmux("send-keys", "-t", "aviary:", "c")
            wait_for("Full context 1")
            tmux("send-keys", "-t", "aviary:", "q")
            wait_for("Preview question 1")
            tmux("send-keys", "-t", "aviary:", "i")
            wait_for("Preview message 1")
            tmux("send-keys", "-t", "aviary:", "j")
            wait_for("Preview message 2")
            tmux("send-keys", "-t", "aviary:", "q", "n")
            wait_for("Work: Preview work 1")
            tmux("send-keys", "-t", "aviary:", "j")
            wait_for("Work: Preview work 2")
            assert all(service.call({"op": "get", "id": item})["answer"] is None for item in ids)
            assert len(service.call({"op": "inbox_page", "participant": "operator"})["messages"]) == 2
            assert len(service.call({"op": "notices_page"})["notices"]) == 2
        finally:
            tmux("kill-server", check=False)
    print("Decision, inbox and notice previews tracked selection without mutations")


if __name__ == "__main__":
    main()
