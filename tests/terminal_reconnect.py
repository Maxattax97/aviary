#!/usr/bin/env python3
"""Exercise terminal recovery across a local service interruption."""

import socket
import subprocess
import time
from pathlib import Path

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


def main() -> None:
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
            terminal.expect("Service unavailable", timeout=6)
            assert terminal.process.poll() is None
            terminal.send("\r")
            terminal.expect("Not confirmed", timeout=16)
            assert terminal.process.poll() is None
            restart(service)
            terminal.expect("draft survives", timeout=8)
            assert service.call({"op": "get", "id": request_id})["answer"] is None
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
            terminal.expect("Service unavailable", timeout=6)
            started = time.monotonic()
            terminal.quit()
            assert time.monotonic() - started < 5
        print(
            "Terminal recovered an unsent draft after restart and exited while offline"
        )


if __name__ == "__main__":
    main()
