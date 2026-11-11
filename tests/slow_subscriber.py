#!/usr/bin/env python3
"""Saturate a real loopback subscriber and reconcile from durable state."""
import ctypes
import json
import os
import socket
import subprocess
import tempfile
import time
from pathlib import Path

from support import BINARY
from terminal_reconnect import listening
from zeromq_interop import Dealer


def main():
    with tempfile.TemporaryDirectory(prefix="aviary-slow-sub-") as directory:
        with socket.socket() as probe:
            probe.bind(("127.0.0.1", 0))
            port = probe.getsockname()[1]
        address = f"127.0.0.1:{port}"
        log_path = Path(directory) / "service.log"
        with log_path.open("w") as log:
            service = subprocess.Popen([str(BINARY), "serve", str(Path(directory) / "state.sqlite"), address], stderr=log, env={**os.environ, "RUST_LOG": "aviary=debug"})
            slow = actor = None
            try:
                deadline = time.monotonic() + 8
                while not listening("127.0.0.1", port):
                    if service.poll() is not None or time.monotonic() >= deadline:
                        raise AssertionError("service did not start")
                    time.sleep(0.02)
                slow = Dealer(address, receive_hwm=1, receive_buffer=1024)
                actor = Dealer(address)
                slow.send("subscribe", {"op": "subscribe_changes"}, version=2)
                assert slow.receive(version=2)["result"]["watching"] is True
                buffer = ctypes.create_string_buffer(4096)
                def receive_actor():
                    size = actor.check(actor.lib.zmq_recv(actor.socket, buffer, len(buffer), 0))
                    assert size < len(buffer)
                    return json.loads(buffer.raw[:size])
                operation = {"op": "create_channel", "channel": {"name": "pressure", "members": ["a", "b"]}}
                marker = "subscriber hint queue saturated; coalescing changes"
                start = time.monotonic()
                with log_path.open() as reader:
                    tail = ""
                    for count in range(1, 120001):
                        actor.send("load", operation, version=2)
                        response = receive_actor()
                        assert response["correlation"] == "load" and "error" not in response["result"]
                        if count % 100 == 0:
                            chunk = tail + reader.read()
                            if marker in chunk:
                                break
                            tail = chunk[-len(marker):]
                        if time.monotonic() - start > 90:
                            raise AssertionError(f"no saturation after {count} requests in 90 seconds")
                    else:
                        raise AssertionError("no saturation after 120000 requests")
                request = {"op": "submit", "request": {"identity": "slow-final", "agent": "a", "question": "Persisted after saturation", "options": []}}
                actor.send("final", request, version=2)
                final = receive_actor()
                assert final["correlation"] == "final" and final["result"]["id"] > 0
                event = slow.receive(version=2)
                assert event["correlation"] == "event" and event["result"]["changed"] is True
                actor.send("read", {"op": "decision_queue"}, version=2)
                current = receive_actor()
                items = current["result"]["requests"]
                assert len(items) == 1 and items[0]["id"] == final["result"]["id"]
                assert items[0]["request"]["question"] == "Persisted after saturation"
                print(f"TCP subscriber queue saturated after {count} accepted operations; independent client and durable reconciliation passed")
            finally:
                if actor is not None:
                    actor.close()
                if slow is not None:
                    slow.close()
                if service.poll() is None:
                    service.terminate()
                service.wait(timeout=8)


if __name__ == "__main__":
    main()
