#!/usr/bin/env python3
"""Exercise terminal recovery across a local service interruption."""

import socket
import subprocess
import time
from pathlib import Path
from types import SimpleNamespace

from support import BINARY, Service, Terminal


def restart(service: Service) -> None:
    service.process = subprocess.Popen(
        [
            str(BINARY),
            "serve",
            str(Path(service.directory.name) / "state.sqlite"),
            service.address,
        ],
        stderr=subprocess.DEVNULL,
    )
    host, port = service.address.rsplit(":", 1)
    service.wait_for(lambda: listening(host, int(port)), "restarted service")


def listening(host: str, port: int) -> bool:
    try:
        with socket.create_connection((host, port), timeout=0.1):
            return True
    except OSError:
        return False


def stop(service: Service) -> None:
    service.process.kill()
    service.process.wait(timeout=5)


def responsive_while_poll_is_waiting() -> None:
    with socket.socket() as listener:
        listener.bind(("127.0.0.1", 0))
        listener.listen()
        listener.settimeout(5)
        address = "127.0.0.1:" + str(listener.getsockname()[1])
        with Terminal(SimpleNamespace(address=address)) as terminal:
            connection, _ = listener.accept()
            with connection:
                started = time.monotonic()
                terminal.send("m")
                terminal.expect("Compose instruction", timeout=0.75)
                assert time.monotonic() - started < 0.75
                terminal.send("\x1b")
                terminal.expect("Loading...")
                terminal.quit()


def main() -> None:
    responsive_while_poll_is_waiting()
    with Service() as service:
        request_id = service.call(
            {
                "op": "submit",
                "request": {
                    "identity": "terminal-reconnect",
                    "agent": "agent",
                    "question": "Recover draft",
                    "options": [],
                },
            }
        )["id"]
        with Terminal(service) as terminal:
            terminal.expect("Recover draft")
            terminal.send("j\rdraft survives")
            terminal.expect("draft survives")
            stop(service)
            terminal.expect("Refresh incomplete", timeout=6)
            assert terminal.process.poll() is None
            terminal.send("\r")
            terminal.expect("Action in progress", timeout=3)
            terminal.send(
                "\r"
            )  # A repeated confirmation while busy must not queue a retry.
            terminal.expect("Unconfirmed", timeout=16)
            assert terminal.process.poll() is None
            restart(service)
            terminal.expect("draft survives", timeout=8)
            time.sleep(0.5)
            assert service.call({"op": "get", "id": request_id})["answer"] is None
            service.call({"op": "set_deferred", "id": request_id, "deferred": True})
            terminal.expect("Request not on this queue page; draft retained")
            terminal.expect("draft survives")
            service.call({"op": "set_deferred", "id": request_id, "deferred": False})
            terminal.expect("draft survives")
            terminal.send("\r")
            service.wait_for(
                lambda: service.call({"op": "get", "id": request_id})["answer"]
                == {"type": "custom", "value": "draft survives"},
                "recovered draft answer",
            )
            terminal.expect("No active requests")
            arriving = service.call(
                {
                    "op": "submit",
                    "request": {
                        "identity": "live-arrival",
                        "agent": "agent",
                        "question": "Arrived while open",
                        "options": ["Yes"],
                    },
                }
            )["id"]
            terminal.expect("Arrived while open")
            service.call({"op": "answer", "id": arriving, "index": 0})
            terminal.expect("No active requests")
            terminal.quit()
        with Terminal(service) as terminal:
            terminal.expect("No active requests")
            stop(service)
            terminal.expect("Refresh incomplete", timeout=6)
            started = time.monotonic()
            terminal.quit()
            assert time.monotonic() - started < 5
        print(
            "Terminal remained responsive during a stalled poll and recovered after restart"
        )


if __name__ == "__main__":
    main()
