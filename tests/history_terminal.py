#!/usr/bin/env python3
"""Exercise continuously navigable request history in the terminal."""

from support import Service, Terminal


def main() -> None:
    with Service() as service:
        service.call(
            {
                "op": "create_channel",
                "channel": {"name": "history-team", "members": ["agent", "operator"]},
            }
        )
        decision_id = service.call(
            {
                "op": "submit",
                "request": {
                    "identity": "history-ui-decision",
                    "agent": "agent",
                    "question": "History review",
                    "options": [],
                    "activity": "history-ui",
                },
            }
        )["id"]
        message_id = service.call(
            {
                "op": "send_message",
                "message": {
                    "identity": "history-ui-message",
                    "channel": "history-team",
                    "sender": "agent",
                    "target": {"kind": "participant", "name": "operator"},
                    "body": "Linked message",
                    "activity": "history-ui",
                },
            }
        )["id"]
        service.call({"op": "send_message", "message": {
            "identity": "history-ui-small", "channel": "history-team", "sender": "agent",
            "target": {"kind": "participant", "name": "operator"},
            "body": "Second linked message", "activity": "history-ui",
        }})
        for number in range(10):
            service.call(
                {
                    "op": "send_message",
                    "message": {
                        "identity": f"history-ui-large-{number}",
                        "channel": "history-team",
                        "sender": "agent",
                        "target": {"kind": "participant", "name": "operator"},
                        "body": f"Large message {number} " + "x" * 120_000,
                        "activity": "history-ui",
                    },
                }
            )
        service.call(
            {
                "op": "submit_notice",
                "notice": {
                    "identity": "history-ui-outcome",
                    "agent": "agent",
                    "work": "History task",
                    "result": "History result",
                    "activity": "history-ui",
                },
            }
        )
        first = service.call(
            {
                "op": "history_page",
                "kind": "decision",
                "id": decision_id,
                "section": "messages",
                "offset": 0,
            }
        )
        next_offset = first["next_offset"]
        assert isinstance(next_offset, int) and next_offset > 0
        second = service.call(
            {
                "op": "history_page",
                "kind": "decision",
                "id": decision_id,
                "section": "messages",
                "offset": next_offset,
            }
        )
        next_marker = second["messages"][0]["message"]["body"].split(" x", 1)[0]
        with Terminal(service) as terminal:
            terminal.expect("History review")
            terminal.send("H")
            terminal.expect("history-ui-decision")
            terminal.send("2")
            screen = terminal.expect("Second linked message")
            assert b"Linked message" in screen and b"Second linked message" in screen
            terminal.send("]")
            terminal.expect(next_marker)
            terminal.send("[")
            terminal.expect("History | messages")
            terminal.send("\x1b[H")
            terminal.expect("Second linked message")
            terminal.send("3")
            terminal.expect("History result")
            terminal.send("q")
            terminal.expect("History review")
            terminal.send("i")
            terminal.expect("Linked message")
            terminal.send("H")
            terminal.expect("history-ui-decision")
            terminal.send("2")
            terminal.expect("Linked message")
            terminal.send("q")
            terminal.expect("Linked message")
            terminal.send("q")
            terminal.expect("History review")
            assert service.call({"op": "get", "id": decision_id})["answer"] is None
            service.call(
                {"op": "answer_custom", "id": decision_id, "value": "Resolved"}
            )
            terminal.expect("No active requests")
            terminal.send("gd0\r")
            terminal.expect("Enter a positive numeric ID")
            terminal.send("\x7f" + str(decision_id) + "\r")
            screen = terminal.expect("history-ui-decision")
            assert b"Resolved" in screen, screen
            terminal.send("q")
            terminal.expect("No active requests")
            terminal.quit()
        page = service.call(
            {
                "op": "history_page",
                "kind": "message",
                "id": message_id,
                "section": "messages",
            }
        )
        assert page["messages"][0]["id"] == message_id
    print("Terminal history: continuous records, long bodies, sections and roots passed")


if __name__ == "__main__":
    main()
