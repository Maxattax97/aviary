#!/usr/bin/env python3
"""Exercise competing decision-queue operations and retained retry identities."""
import json
import threading
from concurrent.futures import ThreadPoolExecutor

from support import Service
from terminal_reconnect import restart, stop


def race(service, *operations):
    barrier = threading.Barrier(len(operations))
    def invoke(operation):
        barrier.wait(timeout=5)
        return service.run(operation)
    with ThreadPoolExecutor(max_workers=len(operations)) as executor:
        return list(executor.map(invoke, operations))


def main():
    with Service() as service:
        snapshots = []
        answer_first = 0
        defer_first = 0
        for number in range(8):
            def submit(suffix):
                return service.call({"op": "submit", "request": {
                    "identity": f"race-{number}-{suffix}", "agent": f"agent-{number}-{suffix}",
                    "question": f"Race {number} {suffix}", "options": [], "importance": 4}})["id"]
            first, second = submit("a"), submit("b")
            queue = service.call({"op": "decision_queue"})["requests"]
            assert [item["id"] for item in queue] == [first, second]
            versions = {item["id"]: item["version"] for item in queue}
            swap = {"op": "swap_decisions", "first": first, "second": second,
                    "first_version": versions[first], "second_version": versions[second]}
            choices = [{**swap, "identity": f"swap-{number}-{suffix}"} for suffix in ("x", "y")]
            outcomes = race(service, *choices)
            assert sorted(result.returncode == 0 for result in outcomes) == [False, True]
            loser = next(result for result in outcomes if result.returncode != 0)
            assert "queue changed" in loser.stderr
            winner = choices[next(index for index, result in enumerate(outcomes) if result.returncode == 0)]
            assert service.call(winner) == {"first": first, "second": second}
            queue = service.call({"op": "decision_queue"})["requests"]
            assert [item["id"] for item in queue] == [second, first]
            assert all(item["version"] == 1 for item in queue)
            defer = {"op": "defer_decision", "id": second, "identity": f"same-defer-{number}"}
            outcomes = race(service, defer, defer)
            assert all(result.returncode == 0 for result in outcomes)
            results = [json.loads(result.stdout) for result in outcomes]
            assert results[0] == results[1]
            events = service.call({"op": "decision_events", "agent": f"agent-{number}-b"})["events"]
            assert len(events) == 1 and events[0]["id"] == results[0]["event_id"]
            queue = service.call({"op": "decision_queue"})["requests"]
            assert [item["id"] for item in queue] == [first, second]
            assert next(item for item in queue if item["id"] == second)["version"] == 2
            service.call({"op": "answer_custom", "id": first, "value": "done"})
            answer = {"op": "answer_custom", "id": second, "value": "resolved"}
            later = {"op": "defer_decision", "id": second, "identity": f"answer-race-{number}"}
            answered, deferred = race(service, answer, later)
            assert answered.returncode == 0
            events = service.call({"op": "decision_events", "agent": f"agent-{number}-b"})["events"]
            if deferred.returncode == 0:
                defer_first += 1
                event = json.loads(deferred.stdout)["event_id"]
                assert len(events) == 2 and events[-1]["id"] == event
                assert service.call(later)["event_id"] == event
            else:
                answer_first += 1
                assert "request not found" in deferred.stderr and len(events) == 1
                assert service.run(later).returncode != 0
            assert service.call({"op": "get", "id": second})["answer"]["value"] == "resolved"
            assert service.call({"op": "decision_queue"})["requests"] == []
            snapshots.append((second, defer, results[0], events, number))
        for number in range(4):
            ids = [service.call({"op": "submit", "request": {
                "identity": f"swap-defer-{number}-{suffix}", "agent": f"swap-defer-agent-{number}",
                "question": suffix, "options": [], "importance": 4}})["id"] for suffix in ("first", "second")]
            page = service.call({"op": "decision_queue"})["requests"]
            versions = {item["id"]: item["version"] for item in page}
            swap = {"op": "swap_decisions", "first": ids[0], "second": ids[1],
                    "first_version": versions[ids[0]], "second_version": versions[ids[1]],
                    "identity": f"swap-versus-defer-{number}"}
            defer = {"op": "defer_decision", "id": ids[0], "identity": f"defer-versus-swap-{number}"}
            swapped, deferred = race(service, swap, defer)
            assert deferred.returncode == 0
            if swapped.returncode != 0:
                assert "queue changed" in swapped.stderr
            assert [item["id"] for item in service.call({"op": "decision_queue"})["requests"]] == list(reversed(ids))
            events = service.call({"op": "decision_events", "agent": f"swap-defer-agent-{number}"})["events"]
            assert len(events) == 1 and events[0]["id"] == json.loads(deferred.stdout)["event_id"]
            for item in ids:
                service.call({"op": "answer_custom", "id": item, "value": "done"})
        inserted = []
        for suffix in ("old-first", "old-second"):
            inserted.append(service.call({"op": "submit", "request": {"identity": f"insertion-{suffix}",
                "agent": "insertion-agent", "question": suffix, "options": [], "importance": 4}})["id"])
        before_insert = service.call({"op": "decision_queue"})["requests"]
        inserted.append(service.call({"op": "submit", "request": {"identity": "insertion-new",
            "agent": "insertion-agent", "question": "new", "options": [], "importance": 4}})["id"])
        operation = {"op": "swap_decisions", "first": inserted[0], "second": inserted[1],
            "first_version": before_insert[0]["version"], "second_version": before_insert[1]["version"],
            "identity": "swap-after-insertion"}
        service.call(operation)
        expected_order = [inserted[1], inserted[0], inserted[2]]
        assert [item["id"] for item in service.call({"op": "decision_queue"})["requests"]] == expected_order
        stop(service)
        restart(service)
        assert [item["id"] for item in service.call({"op": "decision_queue"})["requests"]] == expected_order
        assert service.call(operation) == {"first": inserted[0], "second": inserted[1]}
        for request_id, defer, outcome, events, number in snapshots:
            assert service.call(defer) == outcome
            assert service.call({"op": "decision_events", "agent": f"agent-{number}-b"})["events"] == events
            assert service.call({"op": "get", "id": request_id})["answer"]["value"] == "resolved"
    print(f"Competing swaps, duplicate deferrals and restart recovery passed; answer-first={answer_first}, defer-first={defer_first}")


if __name__ == "__main__":
    main()
