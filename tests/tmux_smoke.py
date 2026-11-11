#!/usr/bin/env python3
"""Exercise a decision in an isolated tmux server and real Aviary service."""

import json
import sqlite3
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
            wait_for_screen("Option 0")
            tmux("send-keys", "-t", "aviary:", *(["j"] * 13))
            screen = wait_for_screen("> 13. Custom answer")
            assert "Decision #" in screen, screen
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
            service.call({"op": "answer", "id": detail_id, "index": 0})
            wait_for_screen("No active requests")
            narrow_id = service.call(
                {
                    "op": "submit",
                    "request": {
                        "identity": "tmux-narrow-recommendation",
                        "agent": "tmux-agent",
                        "question": "Proceed?",
                        "options": ["No", "Yes"],
                        "recommendation": 1,
                        "reason": "Ready",
                        "explanation": "Short explanation",
                    },
                }
            )["id"]
            screen = wait_for_screen("> 1. Yes")
            assert "Active decisions" in screen and "Proceed?" in screen, screen
            assert "Enter confirms" in screen, screen
            assert service.call({"op": "get", "id": narrow_id})["answer"] is None
            tmux("send-keys", "-t", "aviary:", "Enter")
            service.wait_for(
                lambda: service.call({"op": "get", "id": narrow_id})["answer"]
                == {"type": "suggested", "index": 1, "value": "Yes"},
                "narrow recommended answer",
            )
            wait_for_screen("No active requests")
            tmux("resize-window", "-t", "aviary:", "-x", "100", "-y", "15")
            resize_id = service.call(
                {
                    "op": "submit",
                    "request": {
                        "identity": "tmux-resize-context",
                        "agent": "tmux-agent",
                        "question": "Resize context",
                        "options": ["Yes"],
                        "context": "x" * 540 + "ANCHOR-MARKER" + "x" * 400,
                    },
                }
            )["id"]
            wait_for_screen("Resize context")
            tmux("send-keys", "-t", "aviary:", "c")
            wait_for_screen("page 1/")
            tmux("send-keys", "-t", "aviary:", "j", "j", "j")
            screen = wait_for_screen("page 4/")
            assert "ANCHOR-MARKER" in "".join(screen.splitlines()), screen
            tmux("resize-window", "-t", "aviary:", "-x", "40", "-y", "12")
            screen = wait_for_screen("page 6/")
            assert "ANCHOR-MARKER" in "".join(screen.splitlines()), screen
            assert service.call({"op": "get", "id": resize_id})["answer"] is None
            tmux("send-keys", "-t", "aviary:", "q")
            wait_for_screen("Resize context")
            service.call({"op": "answer", "id": resize_id, "index": 0})
            wait_for_screen("No active requests")
            oversized = {
                "identity": "tmux-bad-notice",
                "agent": "agent",
                "work": "work",
                "result": "x" * 3_200_000,
            }
            with sqlite3.connect(
                Path(service.directory.name) / "state.sqlite"
            ) as connection:
                connection.execute(
                    "INSERT INTO completion_notices (identity, payload) VALUES (?, ?)",
                    (oversized["identity"], json.dumps(oversized)),
                )
            warning_id = service.call(
                {
                    "op": "submit",
                    "request": {
                        "identity": "tmux-warning-decision",
                        "agent": "agent",
                        "question": "Proceed?",
                        "options": ["No", "Yes"],
                        "recommendation": 1,
                        "reason": "Ready",
                        "explanation": "Short explanation",
                    },
                }
            )["id"]
            wait_for_screen("> 1. Yes")
            screen = wait_for_screen("Refresh: notices")
            assert "Active decisions" in screen and "Proceed?" in screen, screen
            assert "> 1. Yes" in screen and "Enter confirms" in screen, screen
            assert service.call({"op": "get", "id": warning_id})["answer"] is None
            tmux("send-keys", "-t", "aviary:", "Enter")
            service.wait_for(
                lambda: service.call({"op": "get", "id": warning_id})["answer"]
                == {"type": "suggested", "index": 1, "value": "Yes"},
                "decision with failed notice refresh",
            )
            with sqlite3.connect(
                Path(service.directory.name) / "state.sqlite"
            ) as connection:
                connection.execute(
                    "DELETE FROM completion_notices WHERE identity = ?",
                    (oversized["identity"],),
                )
            wait_for_screen("N:0")
            notice_id = service.call(
                {
                    "op": "submit_notice",
                    "notice": {
                        "identity": "tmux-long-notice-agent",
                        "agent": "a" * 200 + "AGENT-END",
                        "work": "Work marker",
                        "result": "RESULT-END",
                    },
                }
            )["id"]
            wait_for_screen("N:1")
            tmux("send-keys", "-t", "aviary:", "n")
            screen = wait_for_screen(f"Notice #{notice_id}")
            assert "d dismiss" in screen, screen
            for _ in range(3):
                tmux("send-keys", "-t", "aviary:", "j")
            wait_for_screen("AGENT-END")
            for _ in range(3):
                tmux("send-keys", "-t", "aviary:", "j")
            wait_for_screen("RESULT-END")
            tmux("send-keys", "-t", "aviary:", "d")
            service.wait_for(
                lambda: service.call({"op": "list_notices"})["notices"] == [],
                "narrow notice dismissal",
            )
            assert (
                service.call({"op": "list_notices", "include_dismissed": True})[
                    "notices"
                ][0]["id"]
                == notice_id
            )
            tmux("send-keys", "-t", "aviary:", "q")
            wait_for_screen("No active requests")
            long_answer = "z" * 280 + " END-OPTION"
            long_id = service.call(
                {
                    "op": "submit",
                    "request": {
                        "identity": "tmux-long-answer",
                        "agent": "tmux-agent",
                        "question": "Long answer review",
                        "options": [long_answer],
                        "recommendation": 0,
                        "reason": "Ready",
                    },
                }
            )["id"]
            tmux("resize-window", "-t", "aviary:", "-x", "100", "-y", "15")
            wait_for_screen("[rec]")
            tmux("send-keys", "-t", "aviary:", "o")
            wait_for_screen("Answer:")
            tmux("send-keys", "-t", "aviary:", "j")
            wait_for_screen("END-OPTION")
            assert service.call({"op": "get", "id": long_id})["answer"] is None
            tmux("send-keys", "-t", "aviary:", "q")
            wait_for_screen("Long answer review")
            tmux("send-keys", "-t", "aviary:", "Enter")
            service.wait_for(
                lambda: service.call({"op": "get", "id": long_id})["answer"]
                == {"type": "suggested", "index": 0, "value": long_answer},
                "long recommended answer",
            )
            wait_for_screen("No active requests")
            service.call(
                {
                    "op": "create_channel",
                    "channel": {"name": "all", "members": ["operator", "agent"]},
                }
            )
            tmux("resize-window", "-t", "aviary:", "-x", "40", "-y", "12")
            tmux("send-keys", "-t", "aviary:", "?")
            wait_for_screen("Keyboard help 1/3")
            tmux("send-keys", "-t", "aviary:", "j")
            wait_for_screen("Keyboard help 2/3")
            tmux("send-keys", "-t", "aviary:", "j")
            wait_for_screen("Keyboard help 3/3")
            tmux("send-keys", "-t", "aviary:", "Escape")
            wait_for_screen("No active requests")
            tmux("send-keys", "-t", "aviary:", "m")
            wait_for_screen("Compose instruction")
            instruction = "x" * 145 + " END-INSTRUCTION"
            tmux("send-keys", "-t", "aviary:", "-l", instruction)
            wait_for_screen("END-INSTRUCTION")
            tmux("send-keys", "-t", "aviary:", "Enter")
            screen = wait_for_screen("Destination: all")
            assert "Ctrl-S sends" in screen, screen
            assert (
                service.call({"op": "inbox", "participant": "agent"})["messages"] == []
            )
            tmux("send-keys", "-t", "aviary:", "C-s")
            service.wait_for(
                lambda: [
                    item["message"]["body"]
                    for item in service.call({"op": "inbox", "participant": "agent"})[
                        "messages"
                    ]
                ]
                == [instruction],
                "long instruction delivery",
            )
            wait_for_screen("No active requests")
            custom_id = service.call(
                {
                    "op": "submit",
                    "request": {
                        "identity": "tmux-long-custom",
                        "agent": "agent",
                        "question": "Long custom",
                        "options": [],
                    },
                }
            )["id"]
            wait_for_screen("Long custom")
            tmux("send-keys", "-t", "aviary:", "j", "Enter")
            wait_for_screen("Custom answer:")
            custom = "a" * 145 + " END-CUSTOM"
            tmux("send-keys", "-t", "aviary:", "-l", custom)
            wait_for_screen("END-CUSTOM")
            tmux("send-keys", "-t", "aviary:", "C-r")
            wait_for_screen("Custom answer review")
            assert service.call({"op": "get", "id": custom_id})["answer"] is None
            for _ in range(4):
                tmux("send-keys", "-t", "aviary:", "j")
            screen = wait_for_screen("END-CUST")
            assert "END-CUSTOM" in screen.replace("\n", ""), screen
            tmux("send-keys", "-t", "aviary:", "q")
            wait_for_screen("Ctrl-R reviews")
            tmux("send-keys", "-t", "aviary:", "Enter")
            service.wait_for(
                lambda: service.call({"op": "get", "id": custom_id})["answer"]
                == {"type": "custom", "value": custom},
                "long custom answer",
            )
            wait_for_screen("No active requests")
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
