#!/usr/bin/env python3
"""Exercise mixed agent attention inputs in the real terminal."""

from support import Service, Terminal


def main() -> None:
    with Service() as service:
        service.call(
            {
                "op": "create_channel",
                "channel": {
                    "name": "attention",
                    "members": ["alpha", "beta", "operator"],
                },
            }
        )
        alpha = service.call(
            {
                "op": "submit",
                "request": {
                    "identity": "attention-alpha",
                    "agent": "alpha",
                    "question": "Decision from alpha",
                    "options": ["Yes"],
                    "importance": 1,
                },
            }
        )["id"]
        beta = service.call(
            {
                "op": "submit",
                "request": {
                    "identity": "attention-beta",
                    "agent": "beta",
                    "question": "Decision from beta",
                    "options": ["Yes"],
                    "importance": 4,
                },
            }
        )["id"]
        service.call(
            {
                "op": "send_message",
                "message": {
                    "identity": "attention-routine",
                    "channel": "attention",
                    "sender": "alpha",
                    "target": {"kind": "participant", "name": "operator"},
                    "body": "Routine execution log",
                },
            }
        )
        service.call(
            {
                "op": "submit_notice",
                "notice": {
                    "identity": "attention-notice",
                    "agent": "beta",
                    "work": "Completed job",
                    "result": "Finished",
                },
            }
        )
        active = service.call({"op": "list_page"})["requests"]
        assert [item["id"] for item in active] == [beta, alpha]
        assert {item["request"]["agent"] for item in active} == {"alpha", "beta"}
        assert [
            item["message"]["body"]
            for item in service.call({"op": "inbox_page", "participant": "operator"})[
                "messages"
            ]
        ] == ["Routine execution log"]
        assert len(service.call({"op": "notices_page"})["notices"]) == 1
        with Terminal(service) as terminal:
            screen = terminal.expect("Decision from beta")
            assert b"Open completion notices: 1" in screen
            assert b"Routine execution log" not in screen
            terminal.send("l")
            screen = terminal.expect("Decision from alpha")
            assert b"Routine execution log" not in screen
            assert service.call({"op": "get", "id": alpha})["answer"] is None
            assert service.call({"op": "get", "id": beta})["answer"] is None
            terminal.send("i")
            terminal.expect("Routine execution log")
            terminal.send("q")
            terminal.expect("Decision from alpha")
            terminal.send("n")
            screen = terminal.expect("Completed job")
            assert b"Finished" in screen
            terminal.send("q")
            terminal.expect("Decision from alpha")
            terminal.quit()
    print(
        "Attention queue: two agents' decisions, separate routine message and completion notice passed"
    )


if __name__ == "__main__":
    main()
