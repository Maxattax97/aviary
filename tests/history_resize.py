#!/usr/bin/env python3
"""Check history reading position across wide and narrow tmux panes."""
import subprocess
import tempfile
import time
from pathlib import Path

from support import BINARY, Service


def main():
    with tempfile.TemporaryDirectory(prefix="aviary-history-resize-") as directory, Service() as service:
        socket = str(Path(directory) / "tmux.sock")
        def tmux(*args, check=True):
            return subprocess.run(["tmux", "-f", "/dev/null", "-S", socket, *args], text=True, capture_output=True, timeout=5, check=check)
        def screen():
            return tmux("capture-pane", "-p", "-t", "aviary:").stdout
        def wait_for(marker):
            deadline = time.monotonic() + 8
            while time.monotonic() < deadline:
                content = screen()
                if marker in "".join(content.splitlines()):
                    return content
                time.sleep(0.05)
            raise AssertionError(f"missing {marker!r}: {content}")
        decision = service.call({"op": "submit", "request": {
            "identity": "history-resize", "agent": "agent", "question": "History resize",
            "options": [], "context": "鳥e\u0301" * 900 + " HISTORYANCHOR " + "tail" * 500}})["id"]
        try:
            tmux("new-session", "-d", "-s", "aviary", "-x", "100", "-y", "20", str(BINARY), "tui", service.address)
            wait_for("History resize")
            tmux("send-keys", "-t", "aviary:", "H")
            wait_for("History | decisions")
            deadline = time.monotonic() + 12
            while time.monotonic() < deadline:
                content = screen()
                if "HISTORYANCHOR" in "".join(content.splitlines()):
                    break
                tmux("send-keys", "-t", "aviary:", "PageDown")
                time.sleep(0.05)
            else:
                raise AssertionError("history anchor not reached")
            # Put the marker close to the first visible line before narrowing.
            for _ in range(20):
                lines = screen().splitlines()
                joined = "".join(lines)
                pos = joined.find("HISTORYANCHOR")
                row = next((i for i in range(len(lines)) if pos >= 0 and sum(map(len, lines[:i + 1])) > pos), None)
                if row is not None and row <= 3:
                    break
                tmux("send-keys", "-t", "aviary:", "Down")
                time.sleep(0.04)
            else:
                raise AssertionError(f"history marker did not approach top of screen: {screen()!r}")
            tmux("resize-window", "-t", "aviary:", "-x", "40", "-y", "12")
            wait_for("HISTORYANCHOR")
            tmux("resize-window", "-t", "aviary:", "-x", "100", "-y", "20")
            wait_for("HISTORYANCHOR")
            assert service.call({"op": "get", "id": decision})["answer"] is None
        finally:
            tmux("kill-server", check=False)
    print("History reading anchor survived wide/narrow resize without answering")


if __name__ == "__main__":
    main()
