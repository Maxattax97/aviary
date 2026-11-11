#!/usr/bin/env python3
"""Check that one unreadable legacy page does not hide other terminal views."""

import json
from pathlib import Path
import sqlite3

from support import Service, Terminal


def main() -> None:
    with Service() as service:
        database = Path(service.directory.name) / "state.sqlite"
        request_id = service.call(
            {
                "op": "submit",
                "request": {
                    "identity": "partial-refresh-decision",
                    "agent": "agent",
                    "question": "Decision remains visible",
                    "options": ["Yes"],
                },
            }
        )["id"]
        oversized = "x" * 3_200_000
        notice = {
            "identity": "legacy-notice",
            "agent": "agent",
            "work": "work",
            "result": oversized,
        }
        with sqlite3.connect(database) as connection:
            connection.execute(
                "INSERT INTO completion_notices (identity, payload) VALUES (?, ?)",
                (notice["identity"], json.dumps(notice)),
            )
        with Terminal(service) as terminal:
            screen = terminal.expect("Decision remains visible", timeout=12)
            assert b"Refresh incomplete: notices:" in screen, screen
            with sqlite3.connect(database) as connection:
                connection.execute(
                    "DELETE FROM completion_notices WHERE identity = ?",
                    (notice["identity"],),
                )
            # Direct SQL is a fixture-only external mutation. A service action
            # wakes subscribed clients so they reconcile durable state.
            service.call({"op": "set_importance", "id": request_id, "importance": 0})
            terminal.expect("notices 0", timeout=12)
            service.call({"op": "answer", "id": request_id, "index": 0})
            terminal.expect("No decisions")
            service.call(
                {
                    "op": "create_channel",
                    "channel": {"name": "ops", "members": ["agent", "operator"]},
                }
            )
            service.call(
                {
                    "op": "send_message",
                    "message": {
                        "identity": "partial-refresh-message",
                        "channel": "ops",
                        "sender": "agent",
                        "target": {"kind": "participant", "name": "operator"},
                        "body": "Independent inbox",
                    },
                }
            )
            legacy = {
                "identity": "legacy-decision",
                "agent": "agent",
                "question": oversized,
                "options": [],
            }
            with sqlite3.connect(database) as connection:
                connection.execute(
                    "INSERT INTO requests (identity, payload) VALUES (?, ?)",
                    (legacy["identity"], json.dumps(legacy)),
                )
            service.call({"op": "create_channel", "channel": {"name": "wake", "members": ["agent"]}})
            terminal.expect("Refresh incomplete: decisions:", timeout=12)
            terminal.send("i")
            terminal.expect("Independent inbox", timeout=12)
            terminal.send("q")
            terminal.quit()
    print("Unreadable legacy pages did not hide independent decision or inbox views")


if __name__ == "__main__":
    main()
