#!/usr/bin/env python3
"""Check focused list navigation and confirmation in a rendered terminal."""
import os

from support import Service, Terminal


def main() -> None:
    with Service() as service:
        ids = []
        for number in (1, 2):
            ids.append(service.call({"op": "submit", "request": {
                "identity": f"layout-{number}", "agent": "agent", "question": f"Layout item {number}",
                "options": ["Yes"], "recommendation": 0, "reason": "Ready",
            }})["id"])
        for monochrome in (False, True):
            env = os.environ.copy()
            env["TERM"] = "xterm"
            if monochrome:
                env["NO_COLOR"] = "1"
            else:
                env.pop("NO_COLOR", None)
            with Terminal(service, env=env) as terminal:
                chunks = []
                feed = terminal._feed_screen
                def capture(chunk):
                    chunks.append(chunk)
                    feed(chunk)
                terminal._feed_screen = capture
                terminal.expect("Decision #1 [FOCUS]")
                output = b"".join(chunks)
                assert b"\x1b[38;5;" not in output
                assert (b"\x1b[36" in output) != monochrome
                terminal.quit()
        env = os.environ.copy()
        env["NO_COLOR"] = "1"
        with Terminal(service, env=env) as terminal:
            terminal.expect("Decision #1 [FOCUS]")
            terminal.send("\t")
            terminal.expect("Decisions [FOCUS]")
            terminal.send("j")
            terminal.expect("Decision #2")
            terminal.send("\r")
            assert all(service.call({"op": "get", "id": item})["answer"] is None for item in ids)
            notice = service.call({"op": "submit_notice", "notice": {
                "identity": "selection-notice", "agent": "agent", "work": "Work", "result": "Done",
            }})["id"]
            terminal.send("n")
            terminal.expect(f"Notice #{notice}")
            terminal.send("d")
            terminal.expect("Notice dismissed")
            terminal.send("q")
            screen = terminal.expect("Decision #2")
            assert b"Decisions [FOCUS]" in screen
            terminal.send("\t")
            terminal.expect("Decision #2 [FOCUS]")
            terminal.send("\r")
            service.wait_for(lambda: service.call({"op": "get", "id": ids[1]})["answer"] is not None, "explicit answer")
            assert service.call({"op": "get", "id": ids[0]})["answer"] is None
            terminal.quit()
    print("Rendered ANSI/monochrome color, focus and explicit confirmation passed")


if __name__ == "__main__":
    main()
