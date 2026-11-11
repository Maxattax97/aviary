#!/usr/bin/env python3
"""Search retained messages and open the selected result in a real terminal."""
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
            terminal.expect("Search messages")
            terminal.send("NEEDLE\r")
            terminal.expect("needle-00")
            terminal.send("n")
            terminal.expect("needle-25")
            terminal.send("j")
            terminal.expect("> alpha: needle-26")
            terminal.send("\r")
            terminal.expect('"body":"needle-26"')
            terminal.send("q")
            terminal.expect("> alpha: needle-26")
            terminal.send("p")
            terminal.expect("needle-00")
            terminal.send("e")
            terminal.send("\x7f" * 6 + "unmatched\r")
            terminal.expect("No matching messages")
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
    print("Terminal search, result paging, opening history and no-match behavior passed")


if __name__ == "__main__":
    main()
