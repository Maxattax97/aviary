#!/usr/bin/env python3
"""Exercise an off-page decision arrival without discarding a draft."""

from support import Service, Terminal


def main() -> None:
    with Service() as service:
        ids = []
        for number in range(9):
            ids.append(
                service.call(
                    {
                        "op": "submit",
                        "request": {
                            "identity": f"arrival-{number}",
                            "agent": "agent",
                            "question": f"Queue item {number}",
                            "options": [],
                            "context": "x" * 120_000,
                        },
                    }
                )["id"]
            )
        original = service.call({"op": "list_page"})
        assert "newest_id" not in original
        status = service.call({"op": "list_page", "include_newest": True})
        assert status["newest_id"] == ids[-1]
        with Terminal(service) as terminal:
            terminal.expect("Queue item 0")
            terminal.send("l" * 6)
            terminal.expect("Queue item 6")
            terminal.send("l")
            terminal.expect("Queue item 7")
            terminal.send("j\rdraft")
            terminal.expect("Custom answer: draft")
            new_id = service.call(
                {
                    "op": "submit",
                    "request": {
                        "identity": "arrival-urgent",
                        "agent": "agent",
                        "question": "New urgent",
                        "options": [],
                        "importance": 5,
                    },
                }
            )["id"]
            terminal.expect("New decision received", timeout=12)
            terminal.send("!")
            terminal.expect("Custom answer: draft!")
            assert service.call({"op": "get", "id": ids[7]})["answer"] is None
            terminal.send("\r")
            service.wait_for(
                lambda: service.call({"op": "get", "id": ids[7]})["answer"]
                == {"type": "custom", "value": "draft!"},
                "retained draft answer",
            )
            terminal.expect("New urgent", timeout=12)
            terminal.expect("New urgent")
            terminal.quit()
        status = service.call({"op": "decision_queue"})
        assert status["newest_id"] == new_id
        assert status["requests"][0]["id"] == new_id
    print("Off-page arrival signaled while the original custom draft was preserved")


if __name__ == "__main__":
    main()
