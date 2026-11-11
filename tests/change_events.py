#!/usr/bin/env python3
"""Verify idle clients do not poll and wake after durable changes."""

import json
import os
from pathlib import Path
import socket
import select
import subprocess
import tempfile
import time
from types import SimpleNamespace

from support import BINARY, Terminal


def main() -> None:
    with tempfile.TemporaryDirectory(prefix="aviary-events-") as directory:
        with socket.socket() as port:
            port.bind(("127.0.0.1", 0))
            address = f"127.0.0.1:{port.getsockname()[1]}"
        log_path = Path(directory) / "service.log"
        env = dict(os.environ, RUST_LOG="aviary=debug")
        with log_path.open("w") as log:
            service = subprocess.Popen([str(BINARY), "serve", str(Path(directory) / "db.sqlite"), address], stderr=log, env=env)
            waiter = None
            watcher = None
            try:
                deadline = time.monotonic() + 8
                while time.monotonic() < deadline:
                    if service.poll() is not None:
                        raise AssertionError("service exited during startup")
                    try:
                        with socket.create_connection(("127.0.0.1", int(address.rsplit(":", 1)[1])), timeout=0.1):
                            break
                    except OSError:
                        time.sleep(0.02)
                else:
                    raise AssertionError("service did not start")
                def call(operation: dict) -> dict:
                    result = subprocess.run([str(BINARY), "call", address, json.dumps(operation)], text=True, capture_output=True, check=True, timeout=10)
                    return json.loads(result.stdout)
                request_id = call({"op": "submit", "request": {"identity": "event-test", "agent": "agent", "question": "Wait for an event", "options": []}})["id"]
                waiter = subprocess.Popen([str(BINARY), "wait", address, str(request_id)], stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
                watcher = subprocess.Popen([str(BINARY), "watch", address], stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, bufsize=1)
                assert watcher.stdout is not None
                assert select.select([watcher.stdout], [], [], 8)[0], "watch did not connect"
                assert json.loads(watcher.stdout.readline())["connected"] is True
                with Terminal(SimpleNamespace(address=address)) as terminal:
                    terminal.expect("Wait for an event")
                    # Allow initial subscription and reconciliation to finish.
                    time.sleep(1.0)
                    count = log_path.read_text().count("processing request")
                    time.sleep(1.6)
                    assert log_path.read_text().count("processing request") == count, "idle client issued periodic reads"
                    assert waiter.poll() is None
                    call({"op": "defer_decision", "id": request_id, "identity": "event-defer"})
                    stdout, stderr = waiter.communicate(timeout=8)
                    assert waiter.returncode == 0, stderr
                    assert select.select([watcher.stdout], [], [], 8)[0], "watch did not receive a change hint"
                    assert json.loads(watcher.stdout.readline())["connected"] is True
                    notice = json.loads(stdout)
                    assert notice["id"] == request_id and notice["deferred_event"] > 0
                    terminal.expect("Wait for an event")
                    terminal.quit()
                service.terminate()
                service.wait(timeout=8)
                assert select.select([watcher.stdout], [], [], 8)[0], "watch did not report disconnect"
                assert json.loads(watcher.stdout.readline())["connected"] is False
                service = subprocess.Popen([str(BINARY), "serve", str(Path(directory) / "db.sqlite"), address], stderr=log, env=env)
                deadline = time.monotonic() + 8
                while time.monotonic() < deadline:
                    try:
                        with socket.create_connection(("127.0.0.1", int(address.rsplit(":", 1)[1])), timeout=0.1):
                            break
                    except OSError:
                        time.sleep(0.02)
                else:
                    raise AssertionError("restarted service did not listen")
                later = call({"op": "submit", "request": {"identity": "while-reconnecting", "agent": "agent", "question": "Reconcile after restart", "options": []}})["id"]
                assert select.select([watcher.stdout], [], [], 8)[0], "watch did not reconnect"
                assert json.loads(watcher.stdout.readline())["connected"] is True
                assert later in [item["id"] for item in call({"op": "decision_queue"})["requests"]]
            finally:
                if waiter is not None and waiter.poll() is None:
                    waiter.kill()
                    waiter.communicate()
                if watcher is not None and watcher.poll() is None:
                    watcher.kill()
                    watcher.communicate()
                if service.poll() is None:
                    service.terminate()
                service.wait(timeout=8)
    print("Idle clients do not poll; deferral wakes waiters and watch reconnects")


if __name__ == "__main__":
    main()
