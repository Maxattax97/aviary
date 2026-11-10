#!/usr/bin/env python3
"""Exercise a decision in an isolated tmux server and real Aviary service."""

import subprocess
import tempfile
import time
from pathlib import Path

from support import BINARY, Service


def main() -> None:
    with tempfile.TemporaryDirectory(
        prefix="aviary-tmux-"
    ) as directory, Service() as service:
        socket = str(Path(directory) / "server.sock")

        def tmux(
            *arguments: str, check: bool = True
        ) -> subprocess.CompletedProcess[str]:
            return subprocess.run(
                ["tmux", "-f", "/dev/null", "-S", socket, *arguments],
                text=True,
                capture_output=True,
                check=check,
                timeout=5,
            )

        def wait_for_screen(marker: str) -> str:
            deadline = time.monotonic() + 8
            while time.monotonic() < deadline:
                result = tmux("capture-pane", "-p", "-t", "aviary:", check=False)
                if result.returncode == 0 and marker in result.stdout:
                    return result.stdout
                time.sleep(0.05)
            raise AssertionError(f"Missing {marker!r} in tmux pane: {result!r}")

        request_id = service.call(
            {
                "op": "submit",
                "request": {
                    "identity": "tmux-decision",
                    "agent": "tmux-agent",
                    "question": "Approve tmux check?",
                    "options": ["No", "Yes"],
                    "recommendation": 1,
                    "reason": "Ready",
                },
            }
        )["id"]
        try:
            tmux(
                "new-session",
                "-d",
                "-s",
                "aviary",
                "-x",
                "100",
                "-y",
                "30",
                str(BINARY),
                "tui",
                service.address,
            )
            screen = wait_for_screen("Approve tmux check?")
            assert "> 1. Yes (recommended: Ready)" in screen, screen
            assert service.call({"op": "get", "id": request_id})["answer"] is None
            tmux("send-keys", "-t", "aviary:", "Enter")
            service.wait_for(
                lambda: service.call({"op": "get", "id": request_id})["answer"]
                == {"type": "suggested", "index": 1, "value": "Yes"},
                "tmux decision",
            )
            wait_for_screen("No active requests")
            many_id = service.call(
                {
                    "op": "submit",
                    "request": {
                        "identity": "tmux-many-options",
                        "agent": "tmux-agent",
                        "question": "Choose from many",
                        "options": [f"Option {number}" for number in range(12)],
                    },
                }
            )["id"]
            wait_for_screen("Choose from many")
            tmux("resize-window", "-t", "aviary:", "-x", "100", "-y", "15")
            wait_for_screen("Choices 1-5 of 13")
            tmux("send-keys", "-t", "aviary:", *(["j"] * 13))
            screen = wait_for_screen("> 13. Custom answer")
            assert "Choices" in screen, screen
            tmux("send-keys", "-t", "aviary:", "Enter")
            wait_for_screen("Custom answer:")
            tmux("send-keys", "-t", "aviary:", "custom text", "Enter")
            service.wait_for(
                lambda: service.call({"op": "get", "id": many_id})["answer"]
                == {"type": "custom", "value": "custom text"},
                "tmux custom answer",
            )
            wait_for_screen("No active requests")
            detail_id = service.call(
                {
                    "op": "submit",
                    "request": {
                        "identity": "tmux-narrow-context",
                        "agent": "tmux-agent",
                        "question": "Narrow detail",
                        "options": ["Yes"],
                        "explanation": "Why",
                        "context": "x" * 280 + " END-CONTEXT",
                    },
                }
            )["id"]
            tmux("resize-window", "-t", "aviary:", "-x", "40", "-y", "12")
            wait_for_screen("Narrow detail")
            tmux("send-keys", "-t", "aviary:", "c")
            screen = wait_for_screen("page 1/")
            assert "j/k pages" in screen, screen
            for _ in range(4):
                tmux("send-keys", "-t", "aviary:", "j")
            screen = wait_for_screen("END-CONTEXT")
            assert "j/k pages" in screen, screen
            assert service.call({"op": "get", "id": detail_id})["answer"] is None
            tmux("send-keys", "-t", "aviary:", "q")
            wait_for_screen("Narrow detail")
            tmux("send-keys", "-t", "aviary:", "q")
            deadline = time.monotonic() + 5
            while time.monotonic() < deadline:
                if tmux("has-session", "-t", "aviary", check=False).returncode != 0:
                    break
                time.sleep(0.05)
            else:
                raise AssertionError("tmux client did not exit")
        finally:
            tmux("kill-server", check=False)
        print("tmux: decisions and narrow context passed; client exited")


if __name__ == "__main__":
    main()
