#!/usr/bin/env python3
"""Exercise focused wheel input through an isolated tmux client."""

from __future__ import annotations

import fcntl
import os
import pty
import select
import signal
import struct
import subprocess
import tempfile
import time
import termios
from pathlib import Path

from support import BINARY, Service


def main() -> None:
    if not shutil_which("tmux"):
        print("tmux unavailable; wheel-in-tmux check skipped")
        return

    with tempfile.TemporaryDirectory(prefix="aviary-wheel-tmux-") as directory, Service() as service:
        socket = str(Path(directory) / "server.sock")

        def tmux(*arguments: str, check: bool = True) -> subprocess.CompletedProcess[str]:
            return subprocess.run(
                ["tmux", "-f", "/dev/null", "-S", socket, *arguments],
                text=True,
                capture_output=True,
                check=check,
                timeout=5,
            )

        def selected_list_row(screen: str) -> str:
            return next(line[:35] for line in screen.splitlines() if line.startswith("│>"))

        request_ids = []
        for number in range(8):
            request_ids.append(
                service.call(
                    {
                        "op": "submit",
                        "request": {
                            "identity": f"tmux-wheel-{number}",
                            "agent": "tmux-wheel-agent",
                            "question": "Tmux wheel detail" if number == 0 else f"Tmux list {number}",
                            "context": " ".join(f"TMUX-{line:03}" for line in range(160)),
                            "options": ["Approve"],
                            "importance": 5 if number == 0 else 0,
                        },
                    }
                )["id"]
            )

        tmux("new-session", "-d", "-s", "wheel", "-x", "100", "-y", "30", str(BINARY), "tui", service.address)
        tmux("set-option", "-g", "mouse", "on")

        env = os.environ.copy()
        env["TERM"] = "xterm-256color"
        env.pop("TMUX", None)
        pid, client_fd = pty.fork()
        if pid == 0:
            os.execvpe(
                "tmux",
                ["tmux", "-f", "/dev/null", "-S", socket, "attach-session", "-t", "wheel"],
                env,
            )
        fcntl.ioctl(client_fd, termios.TIOCSWINSZ, struct.pack("HHHH", 30, 100, 0, 0))
        try:
            def capture() -> str:
                return tmux("capture-pane", "-p", "-t", "wheel:").stdout

            def drain() -> None:
                while select.select([client_fd], [], [], 0)[0]:
                    os.read(client_fd, 65536)

            def wait_for(marker: str, timeout: float = 8) -> str:
                deadline = time.monotonic() + timeout
                while time.monotonic() < deadline:
                    drain()
                    screen = capture()
                    if marker in screen:
                        return screen
                    time.sleep(0.05)
                raise AssertionError(f"Missing {marker!r} in tmux pane: {capture()!r}")

            def send(data: bytes) -> None:
                os.write(client_fd, data)

            def wheel(down: bool, column: int, row: int) -> None:
                button = 65 if down else 64
                send(f"\x1b[<{button};{column + 1};{row + 1}M".encode())

            def wait_change(before: str, timeout: float = 2) -> str:
                deadline = time.monotonic() + timeout
                while time.monotonic() < deadline:
                    drain()
                    current = capture()
                    if current != before:
                        return current
                    time.sleep(0.05)
                raise AssertionError("tmux pane did not change after wheel input")

            def wheel_until_boundary(down: bool, column: int, row: int) -> str:
                for _ in range(80):
                    before = capture()
                    wheel(down, column, row)
                    time.sleep(0.05)
                    drain()
                    current = capture()
                    if current == before:
                        return current
                raise AssertionError("tmux wheel input did not reach a boundary")

            initial = wait_for("Tmux wheel detail")
            assert "From: tmux-wheel-agent" in initial, initial
            wheel(True, 70, 8)
            after_detail = wait_change(initial)
            assert "From: tmux-wheel-agent" not in after_detail, after_detail
            assert service.call({"op": "get", "id": request_ids[0]})["answer"] is None
            wheel(False, 70, 8)
            initial = wait_change(after_detail)
            assert "From: tmux-wheel-agent" in initial, initial
            wheel(True, 70, 8)
            after_detail = wait_change(initial)
            assert "From: tmux-wheel-agent" not in after_detail, after_detail
            boundary = wheel_until_boundary(True, 70, 8)
            wheel(True, 70, 8)
            time.sleep(0.15)
            assert capture() == boundary
            upper = wheel_until_boundary(False, 70, 8)
            wheel(False, 70, 8)
            time.sleep(0.15)
            assert capture() == upper

            send(b"\t")
            before_list = wait_for("[Decisions]")
            selected_before = selected_list_row(before_list)
            wheel(True, 5, 8)
            after_list = wait_change(before_list)
            assert selected_list_row(after_list) != selected_before
            boundary = wheel_until_boundary(True, 5, 8)
            wheel(True, 5, 8)
            time.sleep(0.15)
            assert capture() == boundary
            upper = wheel_until_boundary(False, 5, 8)
            wheel(False, 5, 8)
            time.sleep(0.15)
            assert capture() == upper
            tmux("set-option", "-g", "mouse", "off")
            before_keyboard = capture()
            send(b"j")
            wait_change(before_keyboard)
            assert all(
                service.call({"op": "get", "id": request_id})["answer"] is None
                for request_id in request_ids
            )
        finally:
            tmux("detach-client", "-t", "wheel", check=False)
            try:
                os.kill(pid, signal.SIGTERM)
            except ProcessLookupError:
                pass
            try:
                os.waitpid(pid, 0)
            except ChildProcessError:
                pass
            try:
                tmux("kill-server", check=False)
            except (subprocess.CalledProcessError, subprocess.TimeoutExpired):
                pass

    print("Focused detail and list wheel input passed through tmux 3.x")


def shutil_which(command: str) -> str | None:
    from shutil import which

    return which(command)


if __name__ == "__main__":
    main()
