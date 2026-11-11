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
            terminal.send("\t")
            terminal.expect("Decision #2 [FOCUS]")
            terminal.send("\r")
            service.wait_for(lambda: service.call({"op": "get", "id": ids[1]})["answer"] is not None, "explicit answer")
            assert service.call({"op": "get", "id": ids[0]})["answer"] is None
            terminal.quit()
    print("Rendered monochrome focus, selection and explicit confirmation passed")


if __name__ == "__main__":
    main()
