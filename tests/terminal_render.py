#!/usr/bin/env python3
"""Check full terminal contents and restoration over real tmux redraws."""

import subprocess
import tempfile
import time
from pathlib import Path

from support import BINARY, Service


def main() -> None:
    with tempfile.TemporaryDirectory(prefix="aviary-render-") as directory, Service() as service:
        socket = str(Path(directory) / "tmux.sock")

        def tmux(*args: str, check: bool = True) -> subprocess.CompletedProcess[str]:
            return subprocess.run(["tmux", "-f", "/dev/null", "-S", socket, *args],
                                  text=True, capture_output=True, check=check, timeout=5)

        def screen_with(marker: str) -> list[str]:
            deadline = time.monotonic() + 8
            while time.monotonic() < deadline:
                result = tmux("capture-pane", "-p", "-t", "render:", check=False)
                if result.returncode == 0 and marker in result.stdout:
                    return result.stdout.splitlines()
                time.sleep(0.05)
            raise AssertionError(f"Missing {marker!r}: {result.stdout!r}")

        try:
            tmux("new-session", "-d", "-s", "render", "-x", "80", "-y", "24",
                 "sh", "-c", 'printf "\\033[2J\\033[HOLD SCREEN CONTENT THAT MUST NOT LEAK\\nSECOND OLD LINE"; "$1" tui "$2"; printf "\\nRETURNED_TO_SHELL\\n"; sleep 20',
                 "sh", str(BINARY), service.address)
            lines = screen_with("No active requests")
            assert lines[0] == "Open completion notices: 0 (n to view); i inbox; m compose", lines
            assert lines[1] == "No active requests. i inbox; ? help; q quits.", lines
            assert all(not line.strip() for line in lines[2:]), lines
            request_id = service.call({"op": "submit", "request": {"identity": "render-test", "agent": "agent", "question": "Render this decision", "options": ["Yes"], "recommendation": 0, "reason": "Ready"}})["id"]
            lines = screen_with("Render this decision")
            assert not any("OLD SCREEN" in line or "SECOND OLD LINE" in line for line in lines), lines
            assert any(f"Request {request_id}" in line for line in lines), lines
            tmux("send-keys", "-t", "render:", "?")
            screen_with("Keyboard help")
            assert service.call({"op": "get", "id": request_id})["answer"] is None
            tmux("send-keys", "-t", "render:", "Escape")
            lines = screen_with("Render this decision")
            assert any("> 1." in line for line in lines), lines
            tmux("send-keys", "-t", "render:", "Enter")
            service.wait_for(lambda: service.call({"op": "get", "id": request_id})["answer"] is not None, "render decision")
            lines = screen_with("No active requests")
            assert lines[0].startswith("Open completion notices: 0"), lines
            assert lines[1] == f'Accepted: {{"id":{request_id}}}', lines
            assert lines[2] == "No active requests. i inbox; ? help; q quits.", lines
            assert all(not line.strip() for line in lines[3:]), lines
            tmux("send-keys", "-t", "render:", "q")
            lines = screen_with("RETURNED_TO_SHELL")
            assert any("OLD SCREEN CONTENT THAT MUST NOT LEAK" in line for line in lines), lines
            assert any("SECOND OLD LINE" in line for line in lines), lines
        finally:
            tmux("kill-server", check=False)
    print("Terminal redraws cleanly and restores the prior screen")


if __name__ == "__main__":
    main()
