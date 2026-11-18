#!/usr/bin/env python3
"""Exercise terminal mouse navigation without submitting queue actions."""

import os
import signal
import termios
import time
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
        top_marker = "RIGHT_COLUMN_TOP_9b87c3fe"
        bottom_marker = "RIGHT_COLUMN_BOTTOM_9b87c3fe"
        filler = "X" * 15
        expected_copy = f"Question: {top_marker}{filler}\n{bottom_marker}"
        clipboard_dir = Path(service.directory.name) / "clipboard-bin"
        clipboard_dir.mkdir()
        clipboard_file = Path(service.directory.name) / "clipboard.txt"
        helper_pid_file = Path(service.directory.name) / "clipboard-helper.pid"
        clipboard = clipboard_dir / "wl-copy"
        clipboard.write_text(
            "#!/bin/sh\n"
            "cat > \"$AVIARY_TEST_CLIPBOARD\"\n"
            "echo $$ > \"$AVIARY_TEST_CLIPBOARD_PID\"\n"
            "exec sleep 30\n"
        )
        clipboard.chmod(0o755)
        env = os.environ.copy()
        env["PATH"] = f"{clipboard_dir}{os.pathsep}{env.get('PATH', '')}"
        env["WAYLAND_DISPLAY"] = "aviary-test-wayland"
        env["AVIARY_TEST_CLIPBOARD"] = str(clipboard_file)
        env["AVIARY_TEST_CLIPBOARD_PID"] = str(helper_pid_file)
        ids = []
        for name, question in (
            ("mouse-first", f"{top_marker}{filler}{bottom_marker}"),
            ("mouse-second", "mouse-second"),
        ):
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

        try:
            with Terminal(service, env=env) as terminal:
                terminal.expect(top_marker)
                top = locate(terminal, top_marker, minimum_column=30)
                bottom = locate(terminal, bottom_marker, minimum_column=29)
                # Begin at the lower line, then drag up into the list pane.
                # Selection must stay in the original detail content area.
                drag(terminal, (bottom[0] + len(bottom_marker) - 1, bottom[1]), (4, top[1]))
                terminal.expect("Copying selection to clipboard")
                deadline = time.monotonic() + 2
                while not clipboard_file.exists() and time.monotonic() < deadline:
                    time.sleep(0.01)
                copied_text = clipboard_file.read_text()
                assert copied_text == expected_copy, repr(copied_text)
                assert "|" not in copied_text

                # Clipboard ownership can keep the helper alive. The UI must
                # still process mouse and keyboard events while it does.
                click(terminal, 55, 5)
                terminal.expect("[Decision]")
                assert service.call({"op": "get", "id": ids[1]})["answer"] is None

                click(terminal, 4, 3)
                terminal.expect("mouse-second")
                assert service.call({"op": "get", "id": ids[1]})["answer"] is None

                # The header labels navigate directly to inbox and notices.
                click(terminal, 22, 0)
                terminal.expect("Mouse inbox message")
                click(terminal, 36, 0)
                terminal.expect("Mouse notice work")
                terminal.send("\x03")
                assert terminal.process.wait(timeout=2) == 0
                assert termios.tcgetattr(terminal.slave) == terminal.settings
        finally:
            if helper_pid_file.exists():
                try:
                    os.kill(int(helper_pid_file.read_text()), signal.SIGTERM)
                except ProcessLookupError:
                    pass

    print("Detail-pane drag copied clean multiline text; UI stayed responsive during clipboard ownership")


if __name__ == "__main__":
    main()
