#!/usr/bin/env python3
"""Exercise terminal mouse navigation without submitting queue actions."""

from support import Service, Terminal


def click(terminal: Terminal, column: int, row: int) -> None:
    """Send one SGR left-button press at zero-based screen coordinates."""
    terminal.send(f"\x1b[<0;{column + 1};{row + 1}M")


def main() -> None:
    with Service() as service:
        ids = []
        for name in ("mouse-first", "mouse-second"):
            ids.append(
                service.call(
                    {
                        "op": "submit",
                        "request": {
                            "identity": name,
                            "agent": "worker",
                            "question": name,
                            "options": ["Approve", "Reject"],
                            "recommendation": 0,
                            "reason": "Ready",
                        },
                    }
                )["id"]
            )
        service.call(
            {
                "op": "submit_notice",
                "notice": {
                    "identity": "mouse-notice",
                    "agent": "worker",
                    "work": "Mouse notice work",
                    "result": "Mouse notice result",
                },
            }
        )
        service.call(
            {
                "op": "create_channel",
                "channel": {"name": "mouse-channel", "members": ["worker", "operator"]},
            }
        )
        service.call(
            {
                "op": "send_message",
                "message": {
                    "identity": "mouse-inbox",
                    "channel": "mouse-channel",
                    "sender": "worker",
                    "target": {"kind": "participant", "name": "operator"},
                    "body": "Mouse inbox message",
                },
            }
        )

        with Terminal(service) as terminal:
            terminal.expect("mouse-first")
            click(terminal, 4, 3)
            terminal.expect("mouse-second")
            assert service.call({"op": "get", "id": ids[1]})["answer"] is None

            # Clicking the detail pane moves focus without changing selection.
            click(terminal, 55, 5)
            terminal.send("j")
            assert service.call({"op": "get", "id": ids[1]})["answer"] is None

            # The header labels navigate directly to inbox and notices.
            click(terminal, 22, 0)
            terminal.expect("Mouse inbox message")
            click(terminal, 36, 0)
            terminal.expect("Mouse notice work")
            terminal.send("q")
            terminal.expect("mouse-second")
            terminal.quit()

    print("Mouse focus, row selection, inbox/notices navigation, and no-submit behavior passed")


if __name__ == "__main__":
    main()
