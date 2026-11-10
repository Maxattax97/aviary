#!/usr/bin/env python3
"""Exercise a local SQLite write failure with a per-process file-size limit."""

import json
from pathlib import Path
import resource
import signal
import socket
import subprocess
import sys
import tempfile
import time

from support import BINARY


def limited_file_size() -> None:
    # This applies only to the child service, never to the test runner or host.
    signal.signal(signal.SIGXFSZ, signal.SIG_IGN)
    resource.setrlimit(resource.RLIMIT_FSIZE, (262_144, 262_144))


def main() -> None:
    if sys.platform != "linux":
        print("SKIP: file-size fault fixture is exercised only on Linux")
        return
    if not BINARY.is_file():
        raise RuntimeError("Build Aviary first: cargo build --locked")
    with tempfile.TemporaryDirectory(prefix="aviary-storage-limit-") as directory:
        path = Path(directory) / "state.sqlite"
        with socket.socket() as listener:
            listener.bind(("127.0.0.1", 0))
            host, port = listener.getsockname()
        address = f"{host}:{port}"

        def start(limited: bool) -> subprocess.Popen:
            process = subprocess.Popen(
                [str(BINARY), "serve", str(path), address],
                stderr=subprocess.DEVNULL,
                preexec_fn=limited_file_size if limited else None,
            )
            deadline = time.monotonic() + 8
            while time.monotonic() < deadline:
                if process.poll() is not None:
                    raise RuntimeError("test service exited before listening")
                try:
                    with socket.create_connection((host, port), timeout=0.1):
                        return process
                except OSError:
                    time.sleep(0.05)
            process.kill()
            process.wait()
            raise TimeoutError("test service did not start")

        def stop(process: subprocess.Popen) -> None:
            if process.poll() is None:
                process.terminate()
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait(timeout=5)
                raise
            if process.returncode != 0:
                raise RuntimeError(f"test service exited with {process.returncode}")

        def call(operation: dict, *, success: bool = True) -> dict:
            result = subprocess.run(
                [str(BINARY), "call", address, json.dumps(operation)],
                capture_output=True,
                text=True,
                timeout=15,
            )
            if not success:
                if (
                    result.returncode == 0
                    or result.stdout
                    or "disk I/O error" not in result.stderr
                ):
                    raise AssertionError(f"expected storage error, got {result!r}")
                return {}
            if result.returncode:
                raise RuntimeError(result.stderr)
            return json.loads(result.stdout)

        baseline = {
            "op": "submit",
            "request": {
                "identity": "before-limit",
                "agent": "a",
                "question": "Before failure",
                "options": [],
            },
        }
        notice = {
            "op": "submit_notice",
            "notice": {
                "identity": "retry-after-limit",
                "agent": "a",
                "work": "Large write",
                "result": "x" * 100_000,
            },
        }
        process = start(limited=True)
        try:
            baseline_id = call(baseline)["id"]
            call(notice, success=False)
            if process.poll() is not None:
                raise RuntimeError("service exited on the failed write")
        finally:
            stop(process)
        process = start(limited=False)
        try:
            assert call(baseline)["id"] == baseline_id
            assert call({"op": "list_notices"})["notices"] == []
            first = call(notice)["id"]
            assert call(notice)["id"] == first
            notices = call({"op": "notices_page"})["notices"]
            assert len(notices) == 1 and notices[0]["id"] == first
            assert notices[0]["notice"]["result"] == notice["notice"]["result"]
        finally:
            stop(process)
    print(
        "Limited-file write failed; prior acknowledgement and retry recovered after restart"
    )


if __name__ == "__main__":
    main()
