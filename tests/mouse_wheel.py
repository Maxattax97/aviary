#!/usr/bin/env python3
"""Exercise focused terminal mouse-wheel scrolling and safe boundaries."""

from __future__ import annotations

import os
import select
import time

from support import Service, Terminal


def snapshot(terminal: Terminal) -> str:
    return "\n".join("".join(row).rstrip() for row in terminal.screen)


def selected_list_row(screen: str, list_width: int = 28) -> str:
    return next(
        line[:list_width]
        for line in screen.splitlines()
        if line.startswith("│>")
    )


def wheel(terminal: Terminal, down: bool, column: int, row: int) -> None:
    button = 65 if down else 64
    os.write(
        terminal.master,
        f"\x1b[<{button};{column + 1};{row + 1}M".encode(),
    )


def wait_for_change(terminal: Terminal, before: str, timeout: float = 2) -> str:
    deadline = time.monotonic() + timeout
    changed_at: float | None = None
    last = before
    while time.monotonic() < deadline:
        if select.select([terminal.master], [], [], 0.05)[0]:
            terminal._feed_screen(os.read(terminal.master, 65536))
            terminal.awaiting_update = False
        current = snapshot(terminal)
        if current != last:
            changed_at = time.monotonic()
            last = current
        if changed_at is not None and time.monotonic() - changed_at >= 0.12:
            return current
    raise AssertionError(f"Terminal screen did not change from: {before!r}")


def assert_right_pane_only_changed(before: str, after: str) -> None:
    first_lines = before.splitlines()
    second_lines = after.splitlines()
    assert any(left[30:] != right[30:] for left, right in zip(first_lines[2:], second_lines[2:]))
    assert all(left[:28] == right[:28] for left, right in zip(first_lines[2:], second_lines[2:]))


def wheel_until_boundary(
    terminal: Terminal, down: bool, column: int, row: int
) -> str:
    for _ in range(80):
        before = snapshot(terminal)
        wheel(terminal, down, column, row)
        try:
            wait_for_change(terminal, before, timeout=0.4)
        except AssertionError:
            return before
    raise AssertionError("Wheel input did not reach a scroll boundary")


def assert_wheel_inert(
    terminal: Terminal, down: bool, column: int, row: int, expected: str | None = None
) -> None:
    before = snapshot(terminal) if expected is None else expected
    wheel(terminal, down, column, row)
    time.sleep(0.15)
    assert snapshot(terminal) == before


def main() -> None:
    with Service() as service:
        identity = "wheel-detail"
        request_id = service.call(
            {
                "op": "submit",
                "request": {
                    "identity": identity,
                    "agent": "wheel-agent",
                    "question": "Wheel detail check",
                    "context": " ".join(f"DETAIL-{number:03}" for number in range(120)),
                    "options": ["Approve"],
                    "importance": 5,
                },
            }
        )["id"]
        for number in range(7):
            service.call(
                {
                    "op": "submit",
                    "request": {
                        "identity": f"wheel-list-{number}",
                        "agent": "wheel-agent",
                        "question": f"Wheel list {number}",
                        "options": ["Approve"],
                    },
                }
            )
        service.call(
            {
                "op": "create_channel",
                "channel": {
                    "name": "wheel-channel",
                    "members": ["wheel-agent", "operator"],
                },
            }
        )
        for number in range(7):
            service.call(
                {
                    "op": "send_message",
                    "message": {
                        "identity": f"wheel-message-{number}",
                        "channel": "wheel-channel",
                        "sender": "wheel-agent",
                        "target": {"kind": "participant", "name": "operator"},
                        "body": " ".join(f"INBOX-{line:03}" for line in range(100)),
                    },
                }
            )
            service.call(
                {
                    "op": "submit_notice",
                    "notice": {
                        "identity": f"wheel-notice-{number}",
                        "agent": "wheel-agent",
                        "work": f"Wheel notice {number}",
                        "result": " ".join(f"NOTICE-{line:03}" for line in range(100)),
                    },
                }
            )

        with Terminal(service) as terminal:
            terminal.expect("Wheel detail check")
            current = snapshot(terminal)
            assert "DETAIL-000" in current, current

            # A wheel over the unfocused decision list is ignored.
            wheel(terminal, True, 5, 8)
            time.sleep(0.15)
            assert snapshot(terminal) == current

            # A wheel over the focused detail pane scrolls its content only.
            before = current
            wheel(terminal, True, 60, 8)
            current = wait_for_change(terminal, current)
            changes = [
                (number, first, second)
                for number, (first, second) in enumerate(
                    zip(before.splitlines(), current.splitlines())
                )
                if first != second
            ]
            assert changes, current
            assert all(
                first[:28] == second[:28]
                for first, second in zip(before.splitlines()[2:], current.splitlines()[2:])
            ), changes
            assert service.call({"op": "get", "id": request_id})["answer"] is None

            # Wheel-up reverses the content scroll without changing the list.
            before = current
            wheel(terminal, False, 60, 8)
            current = wait_for_change(terminal, before)
            assert "From: wheel-agent" in current, current
            assert_right_pane_only_changed(before, current)

            before = current
            wheel(terminal, True, 60, 8)
            current = wait_for_change(terminal, before)

            # Headers and footers are non-scrollable and cannot activate controls.
            for column, row in ((40, 0), (60, 22), (28, 8)):
                assert_wheel_inert(terminal, True, column, row)

            # The keyboard still scrolls the focused detail content.
            before = snapshot(terminal)
            terminal.send("\x1b[5~")
            current = wait_for_change(terminal, before)
            assert "From: wheel-agent" in current, current

            # The expanded context viewer is paginated; wheel moves one page
            # in either direction and clamps at both page boundaries.
            terminal.send("c")
            terminal.expect("page 1/")
            page_one = snapshot(terminal)
            wheel(terminal, False, 60, 8)
            assert_wheel_inert(terminal, False, 60, 8, page_one)
            wheel(terminal, True, 60, 8)
            page_two = wait_for_change(terminal, page_one)
            assert "page 2/" in page_two, page_two
            boundary = wheel_until_boundary(terminal, True, 60, 8)
            assert_wheel_inert(terminal, True, 60, 8, boundary)
            page_one = wheel_until_boundary(terminal, False, 60, 8)
            assert "page 1/" in page_one, page_one
            assert_wheel_inert(terminal, False, 60, 8, page_one)
            terminal.send("c")
            terminal.expect("Wheel detail check")
            terminal.quit()

        # Move to the last visible detail line, then verify the lower boundary
        # leaves the screen and unanswered request unchanged.
        with Terminal(service) as terminal:
            terminal.expect("Wheel detail check")
            boundary = wheel_until_boundary(terminal, True, 60, 8)
            assert_wheel_inert(terminal, True, 60, 8, boundary)
            upper = wheel_until_boundary(terminal, False, 60, 8)
            assert_wheel_inert(terminal, False, 60, 8, upper)
            assert len(
                service.call({"op": "inbox_page", "participant": "operator"})["messages"]
            ) == 7
            assert service.call({"op": "get", "id": request_id})["answer"] is None

            # Tab focuses the decision list; wheel changes selection without
            # confirming any decision. A list boundary is inert.
            terminal.send("\t")
            terminal.expect("[Decisions]")
            before = snapshot(terminal)
            assert "Wheel detail check" in selected_list_row(before)
            wheel(terminal, True, 5, 8)
            current = wait_for_change(terminal, before)
            assert "Wheel list" in selected_list_row(current)
            boundary = wheel_until_boundary(terminal, True, 5, 8)
            assert_wheel_inert(terminal, True, 5, 8, boundary)
            upper = wheel_until_boundary(terminal, False, 5, 8)
            assert_wheel_inert(terminal, False, 5, 8, upper)
            assert all(
                service.call({"op": "get", "id": number})["answer"] is None
                for number in range(1, 9)
            )
            terminal.quit()

        with Terminal(service) as terminal:
            terminal.expect("Wheel detail check")
            terminal.send("i")
            terminal.expect("Operator inbox")
            terminal.expect("INBOX-000")
            terminal.send("\t")
            terminal.expect("[Messages]")
            before = snapshot(terminal)
            selected_before = selected_list_row(before)
            wheel(terminal, True, 5, 8)
            current = wait_for_change(terminal, before)
            assert selected_list_row(current) != selected_before
            boundary = wheel_until_boundary(terminal, True, 5, 8)
            assert_wheel_inert(terminal, True, 5, 8, boundary)
            upper = wheel_until_boundary(terminal, False, 5, 8)
            assert_wheel_inert(terminal, False, 5, 8, upper)

            terminal.send("\t")
            terminal.expect("[Message")
            before = snapshot(terminal)
            wheel(terminal, True, 60, 8)
            current = wait_for_change(terminal, before)
            assert_right_pane_only_changed(before, current)
            boundary = wheel_until_boundary(terminal, True, 60, 8)
            assert_wheel_inert(terminal, True, 60, 8, boundary)
            upper = wheel_until_boundary(terminal, False, 60, 8)
            assert_wheel_inert(terminal, False, 60, 8, upper)

            # History is a single content pane and uses the same line step.
            terminal.send("H")
            terminal.expect("History | decisions")
            terminal.send("2")
            terminal.expect("History | messages")
            before = snapshot(terminal)
            wheel(terminal, True, 60, 8)
            current = wait_for_change(terminal, before)
            assert current != before
            boundary = wheel_until_boundary(terminal, True, 60, 8)
            assert_wheel_inert(terminal, True, 60, 8, boundary)
            upper = wheel_until_boundary(terminal, False, 60, 8)
            assert_wheel_inert(terminal, False, 60, 8, upper)
            terminal.send("q")
            terminal.expect("Operator inbox")
            terminal.send("q")
            terminal.expect("Wheel detail check")

            terminal.send("n")
            terminal.expect("Completion notice")
            terminal.expect("NOTICE-000")
            terminal.send("\t")
            terminal.expect("[Notices]")
            before = snapshot(terminal)
            selected_before = selected_list_row(before)
            wheel(terminal, True, 5, 8)
            current = wait_for_change(terminal, before)
            assert selected_list_row(current) != selected_before
            boundary = wheel_until_boundary(terminal, True, 5, 8)
            assert_wheel_inert(terminal, True, 5, 8, boundary)
            upper = wheel_until_boundary(terminal, False, 5, 8)
            assert_wheel_inert(terminal, False, 5, 8, upper)
            assert len(service.call({"op": "notices_page"})["notices"]) == 7

            terminal.send("\t")
            terminal.expect("[Notice")
            before = snapshot(terminal)
            wheel(terminal, True, 60, 8)
            current = wait_for_change(terminal, before)
            assert_right_pane_only_changed(before, current)
            boundary = wheel_until_boundary(terminal, True, 60, 8)
            assert_wheel_inert(terminal, True, 60, 8, boundary)
            upper = wheel_until_boundary(terminal, False, 60, 8)
            assert_wheel_inert(terminal, False, 60, 8, upper)
            assert len(service.call({"op": "notices_page"})["notices"]) == 7
            terminal.send("n")
            terminal.expect("Wheel detail check")
            terminal.quit()

        # No mouse event is sent under a minimal terminal declaration; ordinary
        # keyboard navigation continues to move the focused decision list.
        dumb_env = os.environ.copy()
        dumb_env["TERM"] = "dumb"
        with Terminal(service, env=dumb_env) as terminal:
            terminal.expect("Wheel detail check")
            terminal.send("\t")
            terminal.expect("[Decisions]")
            before = snapshot(terminal)
            terminal.send("j")
            current = wait_for_change(terminal, before)
            assert current != before
            terminal.quit()

    print("Focused views, context pages, boundaries, and keyboard wheel checks passed in a direct PTY")


if __name__ == "__main__":
    main()
