#!/usr/bin/env python3
"""Search retained records and open selected results in a real terminal."""
from support import Service, Terminal
from terminal_reconnect import restart, stop


def main() -> None:
    with Service() as service:
        service.call({"op": "create_channel", "channel": {"name": "s", "members": ["alpha", "operator"]}})
        for i in range(27):
            service.call({"op": "send_message", "message": {
                "identity": f"terminal-search-{i}", "channel": "s", "sender": "alpha",
                "target": {"kind": "participant", "name": "operator"},
                "body": f"needle-{i:02}", "context": "Launch", "activity": "shared"}})
        with Terminal(service) as terminal:
            terminal.expect("Decisions")
            terminal.send("/")
            terminal.expect("Search records")
            terminal.send("NEEDLE\r")
            terminal.expect("needle-00")
            terminal.send("n")
            terminal.expect("needle-25")
            terminal.send("j")
            terminal.expect("> Message alpha: needle-26")
            terminal.send("\r")
            terminal.expect('"body":"needle-26"')
            terminal.send("q")
            terminal.expect("> Message alpha: needle-26")
            terminal.send("p")
            terminal.expect("needle-00")
            terminal.send("e")
            terminal.send("\x7f" * 6 + "unmatched\r")
            terminal.expect("No matching records")
            terminal.send("e")
            terminal.send("\x7f" * 9 + "launch\r")
            terminal.expect("needle-00")
            terminal.send("e")
            terminal.send("\x7f" * 6 + "alpha\r")
            terminal.expect("needle-00")
            stop(service)
            terminal.send("e")
            terminal.send("\r")
            terminal.expect("Search unavailable")
            restart(service)
            terminal.send("R")
            terminal.expect("needle-00")
            terminal.send("\x1b")
            terminal.expect("Decisions")
            terminal.quit()
    with Service() as service:
        service.call({"op": "create_channel", "channel": {"name": "cross", "members": ["agent", "operator"]}})
        decision = service.call({"op": "submit", "request": {"identity": "cross-decision", "agent": "agent",
            "question": "cross-kind decision", "options": []}})["id"]
        service.call({"op": "answer_custom", "id": decision, "value": "resolved-key"})
        message = service.call({"op": "send_message", "message": {"identity": "cross-message", "channel": "cross",
            "sender": "agent", "target": {"kind": "participant", "name": "operator"}, "body": "cross-kind message"}})["id"]
        notice = service.call({"op": "submit_notice", "notice": {"identity": "cross-notice", "agent": "agent",
            "work": "cross-kind notice", "result": "special-outcome"}})["id"]
        service.call({"op": "dismiss_notice", "id": notice})
        assert decision == message == notice  # Distinct kinds can have the same ID.
        with Terminal(service) as terminal:
            terminal.expect("No decisions")
            terminal.send("/")
            terminal.expect("Search records")
            terminal.send("cross-kind\r")
            terminal.expect("> Decision agent: cross-kind decision")
            terminal.send("\r")
            terminal.expect("resolved-key")
            terminal.send("q")
            terminal.expect("> Decision agent: cross-kind decision")
            terminal.send("j")
            terminal.expect("> Message agent: cross-kind message")
            terminal.send("j")
            terminal.expect("> Notice agent: cross-kind notice")
            terminal.send("\r")
            terminal.expect("special-outcome")
            terminal.send("q")
            terminal.expect("> Notice agent: cross-kind notice")
            terminal.send("\x1b")
            terminal.expect("No decisions")
            terminal.quit()
    print("Terminal search, cross-kind navigation, pagination and no-match behavior passed")


if __name__ == "__main__":
    main()
