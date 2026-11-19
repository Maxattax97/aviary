#!/usr/bin/env python3
"""Exercise explicit operator submission to shared work from the terminal."""

from support import Service, Terminal


def main() -> None:
    with Service() as service:
        with Terminal(service) as terminal:
            terminal.expect("Decisions")
            terminal.send("w")
            screen = terminal.expect("Compose shared work")
            assert b"shared work queue all" in screen and b"one claim" in screen
            terminal.send("Build the release artifact")
            terminal.expect("Compose shared work")
            assert service.call({"op": "work_page"})["items"] == []
            terminal.send("\r")
            terminal.expect("Shared work sent")
            items = service.call({"op": "work_page"})["items"]
            assert len(items) == 1
            work_id = items[0]["id"]
            detail = service.call({"op": "get_work", "id": work_id})
            assert detail["work"]["body"] == "Build the release artifact"
            assert detail["status"] == "available"
            terminal.quit()
    print("Explicit terminal submission to distinct shared work passed")


if __name__ == "__main__":
    main()
