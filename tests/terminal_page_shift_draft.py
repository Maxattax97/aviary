#!/usr/bin/env python3
"""Keep a custom-answer target through a concurrent queue page shift."""
from support import Service, Terminal


def main() -> None:
    with Service() as service:
        ids = []
        for number in range(10):
            ids.append(service.call({"op": "submit", "request": {
                "identity": f"shift-draft-{number}", "agent": "agent",
                "question": f"Shift target {number}", "options": [],
                "context": "x" * 120_000,
            }})["id"])
        first = service.call({"op": "decision_queue", "offset": 0})
        boundary = first["next_offset"]
        assert boundary is not None and 0 < boundary < len(ids)
        with Terminal(service) as terminal:
            terminal.expect("Shift target 0")
            for number in range(1, boundary + 1):
                terminal.send("l")
                terminal.expect(f"Shift target {number}")
            terminal.send("j\rdraft 鳥")
            terminal.expect("draft 鳥")
            service.call({"op": "answer_custom", "id": ids[0], "value": "other"})
            terminal.expect("Request not on this queue page", timeout=12)
            terminal.send("!")
            terminal.expect("!_")
            terminal.send("\r")
            service.wait_for(
                lambda: service.call({"op": "get", "id": ids[boundary]})["answer"]
                == {"type": "custom", "value": "draft 鳥!"},
                "original page-shifted decision receives its draft",
            )
            assert service.call({"op": "get", "id": ids[boundary + 1]})["answer"] is None
            terminal.expect("Action accepted")
            terminal.quit()
    print("Page shift retained the draft and its original decision target")


if __name__ == "__main__":
    main()
