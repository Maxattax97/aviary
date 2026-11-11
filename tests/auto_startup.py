#!/usr/bin/env python3
"""Exercise automatic startup and instance identity on a disposable local port."""
import json
import fcntl
import struct
import termios
import os
import pty
import select
import signal
import socket
import subprocess
import tempfile
import time
from pathlib import Path

from support import BINARY

ADDRESS = "127.0.0.1:7432"


def call(cwd: str, operation: dict) -> dict:
    result = subprocess.run([str(BINARY), "call", ADDRESS, json.dumps(operation)], cwd=cwd, capture_output=True, text=True, timeout=12, check=True)
    return json.loads(result.stdout)


def launch(cwd: str, address: str | None = None):
    master, slave = pty.openpty()
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 80, 0, 0))
    args = [str(BINARY), "tui"] + ([address] if address else ["--local-address", ADDRESS])
    process = subprocess.Popen(args, cwd=cwd, stdin=slave, stdout=slave, stderr=slave, env={**os.environ, "TERM": "xterm-256color"})
    os.close(slave)
    return process, master


def wait_screen(process, fd):
    data = b""
    deadline = time.monotonic() + 20
    while time.monotonic() < deadline:
        if process.poll() is not None:
            raise AssertionError(f"terminal exited: {data!r}")
        if select.select([fd], [], [], 0.2)[0]:
            data += os.read(fd, 65536)
            if b"Aviary" in data:
                return
    raise AssertionError(f"terminal did not open: {data!r}")


def quit_terminal(process, fd):
    os.write(fd, b"q")
    assert process.wait(timeout=8) == 0
    os.close(fd)


def stop_service(pid, port):
    os.kill(pid, signal.SIGTERM)
    deadline = time.monotonic() + 10
    while time.monotonic() < deadline:
        try:
            with socket.create_connection(("127.0.0.1", port), timeout=0.1):
                time.sleep(0.05)
        except OSError:
            return
    raise AssertionError("managed service did not stop")


def main():
    global ADDRESS
    with socket.socket() as probe:
        probe.bind(("127.0.0.1", 0))
        port = probe.getsockname()[1]
    ADDRESS = f"127.0.0.1:{port}"
    with tempfile.TemporaryDirectory(prefix="aviary-auto-") as cwd, tempfile.TemporaryDirectory(prefix="aviary-other-") as other:
        terminals = []
        pid = None
        try:
            terminals = [launch(cwd), launch(cwd)]
            for process, fd in terminals:
                wait_screen(process, fd)
            info = call(cwd, {"op": "service_info"})
            assert info["service"] == "aviary" and info["database"] == str(Path(cwd, "aviary.sqlite").resolve())
            pid = info["pid"]
            for process, fd in terminals:
                quit_terminal(process, fd)
            terminals.clear()
            assert call(cwd, {"op": "service_info"})["pid"] == pid
            request = {"op": "submit", "request": {"identity": "auto-start", "agent": "worker", "question": "Continues?", "options": []}}
            request_id = call(cwd, request)["id"]
            process, fd = launch(cwd)
            terminals.append((process, fd))
            wait_screen(process, fd)
            assert call(cwd, request)["id"] == request_id
            assert call(cwd, {"op": "service_info"})["pid"] == pid
            quit_terminal(process, fd)
            terminals.clear()
            conflict = subprocess.run([str(BINARY), "tui", "--local-address", ADDRESS], cwd=other, capture_output=True, text=True, timeout=12)
            assert conflict.returncode != 0 and "different database" in conflict.stderr
            assert not Path(other, "aviary.sqlite").exists()
            stop_service(pid, port)
            pid = None
            process, fd = launch(cwd)
            terminals.append((process, fd))
            wait_screen(process, fd)
            pid = call(cwd, {"op": "service_info"})["pid"]
            assert call(cwd, request)["id"] == request_id
            quit_terminal(process, fd)
            terminals.clear()
        finally:
            for process, fd in terminals:
                if process.poll() is None:
                    process.kill()
                    process.wait()
                os.close(fd)
            if pid is not None:
                stop_service(pid, port)
        with socket.socket() as unrelated:
            unrelated.bind(("127.0.0.1", port))
            unrelated.listen()
            conflict = subprocess.run([str(BINARY), "tui", "--local-address", ADDRESS], cwd=other, capture_output=True, text=True, timeout=12)
            assert conflict.returncode != 0 and "Cannot identify service" in conflict.stderr
            assert not Path(other, "aviary.sqlite").exists()
        process, fd = launch(other, ADDRESS)
        try:
            wait_screen(process, fd)
            assert not Path(other, "aviary.sqlite").exists()
            quit_terminal(process, fd)
        finally:
            if process.poll() is None:
                process.kill()
                process.wait()
                os.close(fd)
        Path(other, "aviary.sqlite").mkdir()
        failure = subprocess.run([str(BINARY), "tui", "--local-address", ADDRESS], cwd=other, capture_output=True, text=True, timeout=12)
        assert failure.returncode != 0 and "failed to start" in failure.stderr
        assert Path(other, "aviary.sqlite").is_dir()
        Path(other, "aviary.sqlite").rmdir()
        invalid = subprocess.run([str(BINARY), "tui", "--local-address", "0.0.0.0:7432"], cwd=other, capture_output=True, text=True, timeout=5)
        assert invalid.returncode != 0 and "loopback" in invalid.stderr
    print("Automatic startup, concurrent reuse, persistence and endpoint conflicts passed")


if __name__ == "__main__":
    main()
