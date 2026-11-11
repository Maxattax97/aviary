#!/usr/bin/env python3
"""Verify a managed service survives loss of its controlling terminal."""
import fcntl
import json
import os
import pty
import select
import signal
import socket
import struct
import subprocess
import tempfile
import termios
import time
from pathlib import Path

from support import BINARY


def main():
    with socket.socket() as probe:
        probe.bind(("127.0.0.1", 0))
        port = probe.getsockname()[1]
    address = f"127.0.0.1:{port}"
    with tempfile.TemporaryDirectory(prefix="aviary-hangup-") as cwd:
        master, slave = pty.openpty()
        fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 80, 0, 0))
        def terminal_session():
            os.setsid()
            fcntl.ioctl(slave, termios.TIOCSCTTY, 0)
        process = subprocess.Popen([str(BINARY), "tui", "--local-address", address], cwd=cwd, stdin=slave, stdout=slave, stderr=slave, preexec_fn=terminal_session, env={**os.environ, "TERM": "xterm-256color"})
        os.close(slave)
        pid = None
        def call(operation):
            result = subprocess.run([str(BINARY), "call", address, json.dumps(operation)], cwd=cwd, capture_output=True, text=True, check=True, timeout=12)
            return json.loads(result.stdout)
        try:
            output = b""
            deadline = time.monotonic() + 15
            while b"Aviary" not in output and time.monotonic() < deadline:
                if process.poll() is not None:
                    raise AssertionError(f"terminal exited before startup: {output!r}")
                if select.select([master], [], [], 0.2)[0]:
                    output += os.read(master, 65536)
            assert b"Aviary" in output
            info = call({"op": "service_info"})
            assert info["database"] == str(Path(cwd, "aviary.sqlite").resolve())
            pid = info["pid"]
            assert os.getsid(pid) == pid and os.getsid(pid) != os.getsid(process.pid)
            os.close(master)
            master = None
            process.wait(timeout=10)
            assert call({"op": "service_info"})["pid"] == pid
            request = {"op": "submit", "request": {"identity": "after-hangup", "agent": "worker", "question": "Still running?", "options": []}}
            request_id = call(request)["id"]
            assert call({"op": "get", "id": request_id})["answer"] is None
        finally:
            if master is not None:
                os.close(master)
            if process.poll() is None:
                process.kill()
                process.wait()
            if pid is not None:
                os.kill(pid, signal.SIGTERM)
                deadline = time.monotonic() + 10
                while time.monotonic() < deadline:
                    try:
                        with socket.create_connection(("127.0.0.1", port), timeout=0.1):
                            time.sleep(0.05)
                    except OSError:
                        break
                else:
                    raise AssertionError("service did not stop")
    print("Managed service survived controlling-terminal hangup and accepted work")


if __name__ == "__main__":
    main()
