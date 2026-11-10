"""Shared process and pseudo-terminal helpers for Aviary smoke tests."""

from __future__ import annotations

import json
import os
from pathlib import Path
import pty
import select
import socket
import subprocess
import tempfile
import termios
import time
from types import TracebackType
from typing import Any, Callable

ROOT = Path(__file__).resolve().parent.parent
BINARY = ROOT / "target" / "debug" / "aviary"


class Service:
    """Run an isolated local service and clean it up even when a check fails."""

    def __enter__(self) -> Service:
        if not BINARY.is_file():
            raise RuntimeError("Build Aviary first: cargo build --locked")
        self.directory = tempfile.TemporaryDirectory(prefix="aviary-smoke-")
        with socket.socket() as sock:
            sock.bind(("127.0.0.1", 0))
            port = sock.getsockname()[1]
        self.address = f"127.0.0.1:{port}"
        self.process = subprocess.Popen(
            [
                str(BINARY),
                "serve",
                str(Path(self.directory.name) / "state.sqlite"),
                self.address,
            ],
            stderr=subprocess.DEVNULL,
        )
        try:
            deadline = time.monotonic() + 5
            while time.monotonic() < deadline:
                if self.process.poll() is not None:
                    raise RuntimeError("Test service exited before listening")
                try:
                    with socket.create_connection(("127.0.0.1", port), timeout=0.1):
                        return self
                except OSError:
                    time.sleep(0.02)
            raise TimeoutError("Test service did not start")
        except BaseException:
            self.close()
            raise

    def close(self) -> None:
        if self.process.poll() is None:
            self.process.kill()
        self.process.wait()
        self.directory.cleanup()

    def __exit__(
        self,
        exc_type: type[BaseException] | None,
        exc: BaseException | None,
        traceback: TracebackType | None,
    ) -> None:
        self.close()

    def run(self, operation: dict[str, Any]) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [str(BINARY), "call", self.address, json.dumps(operation)],
            text=True,
            capture_output=True,
            timeout=15,
            check=False,
        )

    def call(self, operation: dict[str, Any]) -> dict[str, Any]:
        result = self.run(operation)
        if result.returncode:
            raise RuntimeError(f"Aviary call failed: {result.stderr.strip()}")
        return json.loads(result.stdout)

    def wait_for(
        self, predicate: Callable[[], bool], description: str, timeout: float = 8
    ) -> None:
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            if predicate():
                return
            time.sleep(0.05)
        raise TimeoutError(f"Timed out waiting for {description}")


class Terminal:
    """Drive the real terminal client on a PTY and restore settings on failure."""

    def __init__(self, service: Service) -> None:
        self.service = service

    def __enter__(self) -> Terminal:
        self.master, self.slave = pty.openpty()
        self.settings = termios.tcgetattr(self.slave)
        try:
            self.process = subprocess.Popen(
                [str(BINARY), "tui", self.service.address],
                stdin=self.slave,
                stdout=self.slave,
                stderr=self.slave,
            )
        except BaseException:
            os.close(self.master)
            os.close(self.slave)
            raise
        return self

    def send(self, text: str) -> None:
        os.write(self.master, text.encode())

    def expect(self, marker: str, timeout: float = 8) -> bytes:
        target = marker.encode()
        output = bytearray()
        deadline = time.monotonic() + timeout
        while target not in output and time.monotonic() < deadline:
            if self.process.poll() is not None:
                raise AssertionError(f"Terminal exited early: {bytes(output)!r}")
            if select.select([self.master], [], [], 0.2)[0]:
                output.extend(os.read(self.master, 65536))
        if target not in output:
            raise AssertionError(
                f"Missing {marker!r} in terminal output: {bytes(output)!r}"
            )
        return bytes(output)

    def quit(self) -> None:
        self.send("q")
        code = self.process.wait(timeout=5)
        if code != 0:
            raise AssertionError(f"Terminal exited with status {code}")
        if termios.tcgetattr(self.slave) != self.settings:
            raise AssertionError("Terminal settings were not restored")

    def __exit__(
        self,
        exc_type: type[BaseException] | None,
        exc: BaseException | None,
        traceback: TracebackType | None,
    ) -> None:
        try:
            if self.process.poll() is None:
                self.process.kill()
            self.process.wait()
            termios.tcsetattr(self.slave, termios.TCSANOW, self.settings)
        finally:
            os.close(self.master)
            os.close(self.slave)
