#!/usr/bin/env python3
"""Exercise long decision option wrapping and selection in a narrow tmux pane."""

import subprocess
import tempfile
import time
from pathlib import Path

from support import BINARY, Service


def main() -> None:
    with tempfile.TemporaryDirectory(prefix="aviary-option-wrap-") as directory, Service() as service:
        socket = str(Path(directory) / "tmux.sock")

        def tmux(*args: str, check: bool = True) -> subprocess.CompletedProcess[str]:
            return subprocess.run(["tmux", "-f", "/dev/null", "-S", socket, *args],
                                  text=True, capture_output=True, check=check, timeout=5)

        def screen_with(marker: str) -> list[str]:
            deadline = time.monotonic() + 8
            while time.monotonic() < deadline:
                result = tmux("capture-pane", "-p", "-t", "options:", check=False)
                if result.returncode == 0 and marker in result.stdout:
                    return result.stdout.splitlines()
                time.sleep(0.05)
            raise AssertionError(f"Missing {marker!r}: {result.stdout!r}")

        try:
            tmux("new-session", "-d", "-s", "options", "-x", "42", "-y", "14",
                 str(BINARY), "tui", service.address)
            screen_with("No decisions")
            option = "A deliberately long choice with readable words, Unicode 鳥, and emoji 👩‍💻 near its end"
            other = "A second long unselected alternative that must stay separate from the first option"
            request_id = service.call({"op": "submit", "request": {
                "identity": "long-option-render", "agent": "agent",
                "question": "Choose the long option", "options": [option, other],
            }})["id"]
            lines = screen_with("Choose the long option")
            assert any("Answers:" in line for line in lines), lines
            tmux("send-keys", "-t", "options:", "j")
            lines = screen_with("> 1.")
            assert any("deliberately long" in line or "choice with" in line for line in lines), lines
            tmux("send-keys", "-t", "options:", "j")
            lines = screen_with("> 2.")
            assert not any("> 1." in line for line in lines), lines
            assert any("A second long" in line for line in lines), lines
            assert service.call({"op": "get", "id": request_id})["answer"] is None
        finally:
            tmux("kill-server", check=False)
    print("Long options wrap and selection moves in a narrow terminal")


if __name__ == "__main__":
    main()
