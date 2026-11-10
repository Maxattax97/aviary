#!/usr/bin/env python3
"""Check terminal restoration on external SIGINT and SIGTERM."""

import signal
import termios

from support import Service, Terminal


def main() -> None:
    with Service() as service:
        request_id = service.call(
            {
                "op": "submit",
                "request": {
                    "identity": "signal-test",
                    "agent": "agent",
                    "question": "Signal draft",
                    "options": [],
                },
            }
        )["id"]
        for signum in (signal.SIGINT, signal.SIGTERM):
            with Terminal(service) as terminal:
                terminal.expect("Signal draft")
                terminal.send("j\rdraft")
                terminal.expect("Custom answer: draft")
                terminal.process.send_signal(signum)
                assert terminal.process.wait(timeout=5) == 0
                assert termios.tcgetattr(terminal.slave) == terminal.settings
            assert service.call({"op": "get", "id": request_id})["answer"] is None
    print(
        "Terminal restored after SIGINT and SIGTERM; unsent drafts were not submitted"
    )


if __name__ == "__main__":
    main()
