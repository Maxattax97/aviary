#!/usr/bin/env python3
"""Exercise real keyboard interactions against an isolated Aviary service."""

from support import Service, Terminal


def submit(service: Service, request: dict) -> int:
    return service.call({"op": "submit", "request": request})["id"]


def queue_ids(service: Service, operation: str) -> list[int]:
    return [item["id"] for item in service.call({"op": operation})["requests"]]


def exercise_answers(service: Service) -> None:
    expected = {
        "recommended": {"type": "suggested", "index": 1, "value": "Yes"},
        "other": {"type": "suggested", "index": 0, "value": "No"},
        "custom": {"type": "custom", "value": "Custom 鳥"},
    }
    for case, answer in expected.items():
        request = {
            "identity": case,
            "agent": "agent",
            "question": f"Choose {case}",
            "options": ["No", "Yes"],
            "recommendation": 1,
            "reason": "Ready",
        }
        if case == "custom":
            request = {
                "identity": case,
                "agent": "agent",
                "question": "Choose custom",
                "options": [],
            }
        request_id = submit(service, request)
        with Terminal(service) as terminal:
            terminal.expect(f"Choose {case}")
            assert service.call({"op": "get", "id": request_id})["answer"] is None
            terminal.send(
                {"recommended": "\r", "other": "2\r", "custom": "j\rCustom 鳥\r"}[case]
            )
            service.wait_for(
                lambda: service.call({"op": "get", "id": request_id})["answer"]
                == answer,
                f"{case} answer",
            )
            terminal.quit()
    print("Recommended, alternate and custom answers passed")


def exercise_deferral(service: Service) -> None:
    request_id = submit(
        service,
        {
            "identity": "deferred",
            "agent": "agent",
            "question": "Defer this",
            "options": ["Yes"],
            "recommendation": 0,
            "reason": "Ready",
            "importance": 1,
        },
    )
    with Terminal(service) as terminal:
        terminal.expect("Defer this")
        terminal.send("d")
        service.wait_for(
            lambda: queue_ids(service, "list_deferred") == [request_id], "deferral"
        )
        terminal.quit()
    with Terminal(service) as terminal:
        terminal.expect("No active requests")
        terminal.send("D")
        terminal.expect("Defer this")
        terminal.send("p5")
        service.wait_for(
            lambda: service.call({"op": "list_deferred"})["requests"][0]["importance"]
            == 5,
            "priority override",
        )
        terminal.expect("importance 5")
        terminal.send("d")
        service.wait_for(
            lambda: queue_ids(service, "list") == [request_id], "restoration"
        )
        terminal.quit()
    service.call({"op": "answer", "id": request_id, "index": 0})
    print("Deferral, reopen, priority override and restoration passed")


def exercise_context_and_notices(service: Service) -> int:
    request_id = submit(
        service,
        {
            "identity": "details",
            "agent": "agent",
            "question": "Detail check",
            "options": ["Yes"],
            "recommendation": 0,
            "reason": "Ready",
            "explanation": "Why this matters",
            "context": "A" * 430 + " END-CONTEXT",
        },
    )
    with Terminal(service) as terminal:
        terminal.expect("Why this matters")
        terminal.send("c")
        terminal.expect("page 1/")
        terminal.send("jj")
        terminal.expect("END-CONTEXT")
        terminal.send("q")
        terminal.expect("Detail check")
        assert service.call({"op": "get", "id": request_id})["answer"] is None
        terminal.quit()
    with Terminal(service) as terminal:
        terminal.expect("Detail check")
        notice_id = service.call(
            {
                "op": "submit_notice",
                "notice": {
                    "identity": "notice-pty",
                    "agent": "worker",
                    "work": "Build package",
                    "result": "Tests passed",
                },
            }
        )["id"]
        terminal.expect("notices 1")
        terminal.send("n")
        screen = terminal.expect("Build package")
        assert b"Tests passed" in screen and f"Notice #{notice_id}".encode() in screen
        terminal.send("?")
        terminal.expect("Keyboard help 1/3")
        terminal.send("j")
        terminal.expect("Notices: Tab focus")
        terminal.send("\x1b")
        terminal.expect(f"Notice #{notice_id}")
        terminal.send("d")
        service.wait_for(
            lambda: not service.call({"op": "list_notices"})["notices"],
            "notice dismissal",
        )
        assert (
            service.call({"op": "list_notices", "include_dismissed": True})["notices"][
                0
            ]["id"]
            == notice_id
        )
        terminal.send("q")
        terminal.expect("Detail check")
        terminal.quit()
    print("Explanation, paged context and live completion notice passed")
    return request_id


def exercise_instructions(service: Service) -> None:
    with Terminal(service) as terminal:
        terminal.expect("Detail check")
        terminal.send("m")
        terminal.expect("Compose instruction")
        terminal.send("Start work?")
        terminal.send("\x07")
        terminal.expect("Keyboard help")
        terminal.send("\x1b")
        assert b"Start work?" in terminal.expect("Compose instruction")
        terminal.send("\r")
        review = terminal.expect("Destination: all")
        assert b"group" in review and b"Start work?" in review
        assert service.call({"op": "inbox", "participant": "a"})["messages"] == []
        terminal.send("\x13")
        terminal.expect("Rejected:")
        service.call({"op": "create_channel", "channel": {"name": "all", "members": ["operator", "a", "b"]}})
        terminal.send("\x13")
        terminal.expect("Instruction sent")
        terminal.send("m")
        terminal.expect("Compose instruction")
        terminal.send("Second instruction\r")
        terminal.expect("Destination: all")
        terminal.send("\x13")
        terminal.expect("Instruction sent")
        inbox_a = service.call({"op": "inbox", "participant": "a"})["messages"]
        inbox_b = service.call({"op": "inbox", "participant": "b"})["messages"]
        assert [item["message"]["body"] for item in inbox_a] == ["Start work?", "Second instruction"]
        assert [item["message"]["body"] for item in inbox_b] == ["Start work?", "Second instruction"]
        assert len({item["message"]["identity"] for item in inbox_a}) == 2
        terminal.quit()
    print("Confirmed group instructions and retry after rejection passed")


def exercise_operator_inbox(service: Service) -> None:
    service.call(
        {
            "op": "create_channel",
            "channel": {"name": "inbox-ops", "members": ["operator", "worker"]},
        }
    )
    with Terminal(service) as terminal:
        terminal.expect("Detail check")
        terminal.quit()
    request_id = service.call(
        {
            "op": "send_message",
            "message": {
                "identity": "operator-request",
                "channel": "inbox-ops",
                "sender": "operator",
                "target": {"kind": "participant", "name": "worker"},
                "body": "Check release",
                "expects_reply": True,
                "context": "Build release",
                "activity": "release-work",
            },
        }
    )["id"]
    service.call(
        {
            "op": "reply_message",
            "reply": {
                "identity": "worker-reply",
                "request_id": request_id,
                "sender": "worker",
                "body": "Ready for review",
            },
        }
    )
    with Terminal(service) as terminal:
        terminal.expect("Detail check")
        terminal.send("i")
        screen = terminal.expect("Ready for review")
        assert b"From: worker" in screen and b"Work: Build release" in screen
        assert b"Activity: release-work" in screen
        assert b"r reply" not in screen
        live = service.call(
            {
                "op": "send_message",
                "message": {
                    "identity": "worker-live",
                    "channel": "inbox-ops",
                    "sender": "worker",
                    "target": {"kind": "participant", "name": "operator"},
                    "body": "Live operator update",
                    "context": "Workstream",
                    "activity": "release-work",
                },
            }
        )["id"]
        terminal.expect(f"#{live}")
        terminal.send("l")
        screen = terminal.expect("Live operator update")
        assert b"From: worker" in screen and b"Work: Workstream" in screen
        incoming = service.call(
            {
                "op": "send_message",
                "message": {
                    "identity": "worker-request",
                    "channel": "inbox-ops",
                    "sender": "worker",
                    "target": {"kind": "participant", "name": "operator"},
                    "body": "Need operator response",
                    "expects_reply": True,
                    "context": "Decision context",
                    "activity": "release-work",
                },
            }
        )["id"]
        terminal.expect(f"#{incoming}")
        terminal.send("l")
        screen = terminal.expect("Need operator response")
        assert f"Message #{incoming}".encode() in screen
        assert b"r reply" in screen
        terminal.send("?")
        terminal.expect("Keyboard help 1/3")
        terminal.send("j")
        terminal.expect("Inbox: Tab focus")
        terminal.send("\x1b")
        terminal.expect(f"Message #{incoming}")
        original_reply_id = None
        for attempt, identity in enumerate(
            ["operator-reply", "operator-reply", "different-reply"]
        ):
            terminal.send("r")
            terminal.expect("Compose reply")
            terminal.send(f"{identity}\rApproved\r")
            review = terminal.expect("Review reply")
            assert (
                identity.encode() in review
                and b"Approved" in review
                and b"To: worker" in review
            )
            if attempt == 0:
                assert (
                    service.call({"op": "get_message_response", "id": incoming})[
                        "response"
                    ]
                    is None
                )
            terminal.send("\r")
            if identity == "different-reply":
                failure = terminal.expect("Rejected:")
                assert b"request already answered" in failure and b"Approved" in failure
                terminal.send("\x1b")
                terminal.expect("Need operator response")
            else:
                terminal.expect("Reply sent")
                reply_id = service.call({"op": "get_message_response", "id": incoming})[
                    "response"
                ]["id"]
                if original_reply_id is None:
                    original_reply_id = reply_id
                else:
                    assert reply_id == original_reply_id
        response = service.call({"op": "get_message_response", "id": incoming})[
            "response"
        ]
        assert response["message"]["identity"] == "operator-reply"
        assert response["message"]["body"] == "Approved"
        assert response["message"]["activity"] == "release-work"
        terminal.send("q")
        terminal.expect("Detail check")
        terminal.quit()
    inbox = service.call({"op": "inbox", "participant": "operator"})["messages"]
    assert len(inbox) == 3
    print("Operator inbox recovered messages and sent a correlated reply with retry")


def exercise_large_queues(service: Service, request_id: int) -> None:
    service.call({"op": "answer", "id": request_id, "index": 0})
    large = "x" * 120_000
    notice_ids = []
    request_ids = []
    inbox_ids = []
    for number in range(10):
        request_ids.append(
            submit(
                service,
                {
                    "identity": f"large-request-{number}",
                    "agent": "agent",
                    "question": f"Queue item {number}",
                    "options": [],
                    "context": large,
                },
            )
        )
        notice_ids.append(
            service.call(
                {
                    "op": "submit_notice",
                    "notice": {
                        "identity": f"large-notice-{number}",
                        "agent": "agent",
                        "work": f"Notice {number}",
                        "result": large,
                    },
                }
            )["id"]
        )
        inbox_ids.append(
            service.call(
                {
                    "op": "send_message",
                    "message": {
                        "identity": f"large-inbox-{number}",
                        "channel": "inbox-ops",
                        "sender": "worker",
                        "target": {"kind": "participant", "name": "operator"},
                        "body": f"Big inbox {number} " + large,
                    },
                }
            )["id"]
        )
    with Terminal(service) as terminal:
        terminal.expect("Queue item 0")
        terminal.send("\t")
        terminal.expect("Decisions [FOCUS]")
        for index in range(1, 8):
            terminal.send("j")
            terminal.expect(f"Decision #{request_ids[index]}")
        terminal.send("l" * 2)
        terminal.expect("Queue item 9")
        terminal.send("h" * 2)
        terminal.expect("Queue item 7")
        terminal.send("k")
        terminal.expect(f"Decision #{request_ids[6]}")
        terminal.send("l")
        terminal.expect("Queue item 7")
        terminal.send("l" * 2)
        terminal.expect("Queue item 9")
        terminal.send("n")
        terminal.expect("Notice 0")
        for index in range(1, 8):
            terminal.send("j")
            terminal.expect(f"Notice #{notice_ids[index]}")
        terminal.send("l" * 2)
        terminal.expect("Notice 9")
        terminal.send("h" * 2)
        terminal.expect("Notice 7")
        terminal.send("k")
        terminal.expect(f"Notice #{notice_ids[6]}")
        terminal.send("l")
        terminal.expect("Notice 7")
        terminal.send("l" * 2)
        terminal.expect("Notice 9")
        for notice_id in notice_ids[:7]:
            service.call({"op": "dismiss_notice", "id": notice_id})
        terminal.expect("notices 3")
        terminal.send("d")
        service.wait_for(
            lambda: notice_ids[-1]
            not in [
                item["id"] for item in service.call({"op": "list_notices"})["notices"]
            ],
            "notice dismissal after page shift",
        )
        terminal.expect("Notice 7")
        terminal.send("q")
        terminal.expect("Queue item 9")
        for earlier_id in request_ids[:7]:
            service.call({"op": "answer_custom", "id": earlier_id, "value": "done"})
        terminal.expect("Active decisions")
        terminal.send("i")
        terminal.expect("Ready for review")
        terminal.send("l" * 9)
        terminal.expect("Big inbox 6")
        terminal.send("j")
        terminal.expect(f"Message #{inbox_ids[7]}")
        terminal.send("l" * 2)
        terminal.expect("Big inbox 9")
        terminal.send("h" * 2)
        terminal.expect("Big inbox 7")
        terminal.send("k")
        terminal.expect(f"Message #{inbox_ids[6]}")
        terminal.send("q")
        terminal.quit()
    print("Terminal navigated decision, notice, and inbox data above frame limit")


def exercise_concurrent_answer() -> None:
    with Service() as service:
        request_id = submit(service, {"identity": "concurrent-answer", "agent": "worker", "question": "Concurrent decision", "options": []})
        with Terminal(service) as terminal:
            terminal.expect("Concurrent decision")
            terminal.send("j\rmy draft")
            terminal.expect("my draft")
            service.call({"op": "answer_custom", "id": request_id, "value": "other answer"})
            terminal.send("\r")
            terminal.expect("Rejected: request already answered")
            assert service.call({"op": "get", "id": request_id})["answer"]["value"] == "other answer"
            terminal.send("\x1b")
            terminal.expect("No active requests")
            terminal.quit()
    print("Concurrent answer is rejected without replacing the existing answer")


def exercise_unsuitable_channel() -> None:
    with Service() as service:
        channel = {"name": "all", "members": ["operator"]}
        service.call({"op": "create_channel", "channel": channel})
        with Terminal(service) as terminal:
            terminal.expect("No active requests")
            terminal.send("m")
            terminal.expect("Compose instruction")
            terminal.send("Cannot deliver\r")
            terminal.expect("Destination: all")
            terminal.send("\x13")
            terminal.expect("Rejected:")
            assert service.call({"op": "create_channel", "channel": channel})["channel"] == channel
            assert service.run({"op": "create_channel", "channel": {"name": "all", "members": ["operator", "worker"]}}).returncode != 0
            assert not service.call({"op": "inbox", "participant": "worker"})["messages"]
            terminal.send("\x1b")
            terminal.expect("Discard this")
            terminal.send("y")
            terminal.expect("No active requests")
            terminal.quit()
    print("Unsuitable all channel is rejected without membership changes")


def main() -> None:
    with Service() as service:
        exercise_answers(service)
        exercise_deferral(service)
        request_id = exercise_context_and_notices(service)
        exercise_instructions(service)
        exercise_operator_inbox(service)
        exercise_large_queues(service, request_id)
    exercise_concurrent_answer()
    exercise_unsuitable_channel()


if __name__ == "__main__":
    main()
