#!/usr/bin/env python3
"""Exercise snapshot overview, change hints and unavailable state."""
import sqlite3
from pathlib import Path

from support import Service, Terminal
from terminal_reconnect import restart, stop


def main() -> None:
    with Service() as service:
        assert service.call({"op": "overview"}) == {"decisions": 0, "messages": 0, "notices": 0}
        service.call({"op": "create_channel", "channel": {"name": "overview", "members": ["alpha", "operator"]}})
        with Terminal(service) as terminal:
            terminal.expect("Decisions")
            terminal.send("v")
            terminal.expect("Pending decisions: 0")
            terminal.expect("Retained operator messages: 0")
            terminal.expect("Open completion notices: 0")
            decision = service.call({"op": "submit", "request": {"identity": "overview-d", "agent": "alpha", "question": "Choose", "options": []}})["id"]
            for i in range(27):
                service.call({"op": "send_message", "message": {"identity": f"overview-m{i}", "channel": "overview", "sender": "alpha", "target": {"kind": "participant", "name": "operator"}, "body": "Message" + "x" * 40000}})
            notice = service.call({"op": "submit_notice", "notice": {"identity": "overview-n", "agent": "alpha", "work": "Work", "result": "Done"}})["id"]
            assert service.call({"op": "inbox_page", "participant": "operator"})["next_offset"] is not None
            terminal.expect("Pending decisions: 1")
            terminal.expect("Retained operator messages: 27")
            terminal.expect("Open completion notices: 1")
            terminal.expect("Messages include previously read items")
            service.call({"op": "answer_custom", "id": decision, "value": "done"})
            service.call({"op": "dismiss_notice", "id": notice})
            terminal.expect("Pending decisions: 0")
            terminal.expect("Open completion notices: 0")
            terminal.expect("Retained operator messages: 27")
            stop(service)
            terminal.expect("Unavailable: live connection disconnected")
            restart(service)
            terminal.expect("Retained operator messages: 27")
            terminal.send("q")
            terminal.expect("Decisions")
            terminal.quit()
    with Service() as service:
        with Terminal(service) as terminal:
            terminal.expect("Decisions")
            terminal.send("v")
            terminal.expect("Open completion notices: 0")
            with sqlite3.connect(Path(service.directory.name) / "state.sqlite") as db:
                db.execute("DROP TABLE completion_notices")
            terminal.send("R")
            terminal.expect("Unavailable:")
            terminal.send("q")
            terminal.expect("Decisions")
            terminal.quit()
    print("Overview snapshot counts, live updates, retained messages and reconnect passed")


if __name__ == "__main__":
    main()
