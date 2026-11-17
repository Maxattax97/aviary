#!/usr/bin/env python3
"""Exercise terminal mouse navigation without submitting queue actions."""

import os
from pathlib import Path

from support import Service, Terminal


def click(terminal: Terminal, column: int, row: int) -> None:
    """Send one SGR left-button click at zero-based screen coordinates."""
    terminal.send(
        f"\x1b[<0;{column + 1};{row + 1}M"
        f"\x1b[<0;{column + 1};{row + 1}m"
    )


def drag(terminal: Terminal, start: tuple[int, int], end: tuple[int, int]) -> None:
    """Drag the left mouse button between zero-based SGR coordinates."""
    start_x, start_y = start
    end_x, end_y = end
    terminal.send(
        f"\x1b[<0;{start_x + 1};{start_y + 1}M"
        f"\x1b[<32;{end_x + 1};{end_y + 1}M"
        f"\x1b[<0;{end_x + 1};{end_y + 1}m"
    )


def locate(terminal: Terminal, marker: str, minimum_column: int = 0) -> tuple[int, int]:
    for row_number, row in enumerate(terminal.screen):
        line = "".join(row)
        column = line.find(marker)
        if column >= minimum_column:
            return column, row_number
    raise AssertionError(f"Could not locate {marker!r} at column {minimum_column} or later")


def main() -> None:
    with Service() as service:
        marker = "RIGHT_COLUMN_COPY_MARKER_9b87c3fe"
        clipboard_dir = Path(service.directory.name) / "clipboard-bin"
        clipboard_dir.mkdir()
        clipboard_file = Path(service.directory.name) / "clipboard.txt"
        clipboard = clipboard_dir / "wl-copy"
        clipboard.write_text("#!/bin/sh\ncat > \"$AVIARY_TEST_CLIPBOARD\"\n")
        clipboard.chmod(0o755)
        env = os.environ.copy()
        env["PATH"] = f"{clipboard_dir}{os.pathsep}{env.get('PATH', '')}"
        env["WAYLAND_DISPLAY"] = "aviary-test-wayland"
        env["AVIARY_TEST_CLIPBOARD"] = str(clipboard_file)
        ids = []
        for name, question in (("mouse-first", marker), ("mouse-second", "mouse-second")):
            ids.append(
                service.call(
                    {
                        "op": "submit",
                        "request": {
                            "identity": name,
                            "agent": "worker",
                            "question": question,
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

        with Terminal(service, env=env) as terminal:
            terminal.expect(marker)
            start = locate(terminal, marker, minimum_column=30)
            end = (start[0] + len(marker) - 1, start[1])
            drag(terminal, start, end)
            terminal.expect("Copied selection to system clipboard")
            assert clipboard_file.read_text() == marker

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

    print("Mouse drag copied only the right-column text; click navigation and no-submit checks passed")


if __name__ == "__main__":
    main()
