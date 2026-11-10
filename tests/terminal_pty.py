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
        terminal.send("\t")
        terminal.expect("Defer this")
        terminal.send("p5")
        service.wait_for(
            lambda: service.call({"op": "list_deferred"})["requests"][0]["importance"]
            == 5,
            "priority override",
        )
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
        terminal.expect("Open completion notices: 1")
        terminal.send("n")
        assert b"Tests passed" in terminal.expect("Build package")
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
    service.call(
        {
            "op": "create_channel",
            "channel": {"name": "ops", "members": ["operator", "a", "b"]},
        }
    )
    with Terminal(service) as terminal:
        terminal.expect("Detail check")
        for identity, target, text in [
            ("instruction-direct", "a", "Review this"),
            ("instruction-group", "", "Start work"),
        ]:
            terminal.send("m")
            terminal.expect("Retry identity")
            terminal.send(f"{identity}\rops\r{target}\r{text}\r")
            review = terminal.expect("Review instruction")
            assert identity.encode() in review and text.encode() in review
            terminal.send("\r")
            terminal.expect("Instruction accepted")
        terminal.send("m")
        terminal.expect("Retry identity")
        terminal.send("instruction-invalid\rmissing-channel\ra\rHold\r")
        terminal.expect("Review instruction")
        terminal.send("\r")
        failure = terminal.expect("Send failed or outcome unknown")
        assert b"instruction-invalid" in failure and b"Hold" in failure
        terminal.send("\r")
        terminal.expect("Send failed or outcome unknown")
        terminal.send("\x1b")
        terminal.expect("Detail check")
        inbox_a = service.call({"op": "inbox", "participant": "a"})["messages"]
        inbox_b = service.call({"op": "inbox", "participant": "b"})["messages"]
        assert [item["message"]["body"] for item in inbox_a] == [
            "Review this",
            "Start work",
        ]
        assert [item["message"]["body"] for item in inbox_b] == ["Start work"]
        assert all(item["message"]["sender"] == "operator" for item in inbox_a)
        terminal.quit()
    print("Direct and group operator instructions passed")


def exercise_large_queues(service: Service, request_id: int) -> None:
    service.call({"op": "answer", "id": request_id, "index": 0})
    large = "x" * 120_000
    for number in range(10):
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
        )
    with Terminal(service) as terminal:
        terminal.expect("Queue item 0")
        terminal.send("l" * 9)
        terminal.expect("Queue item 9")
        terminal.send("n")
        terminal.expect("Notice 0")
        terminal.send("l" * 9)
        terminal.expect("Notice 9")
        terminal.send("q")
        terminal.quit()
    print("Terminal navigated decision and notice queues above frame limit")


def main() -> None:
    with Service() as service:
        exercise_answers(service)
        exercise_deferral(service)
        request_id = exercise_context_and_notices(service)
        exercise_instructions(service)
        exercise_large_queues(service, request_id)


if __name__ == "__main__":
    main()
