#!/usr/bin/env python3
"""Check durable queue ordering, deferral events, and stale-safe swaps."""

import json
import subprocess
import time
import sqlite3
from pathlib import Path

from support import BINARY, Service
from terminal_reconnect import restart, stop


def main() -> None:
    with Service() as service:
        def submit(name: str, importance: int) -> int:
            return service.call({"op": "submit", "request": {
                "identity": f"queue-{name}", "agent": name,
                "question": f"Question {name}", "options": [], "importance": importance,
            }})["id"]

        first = submit("first", 4)
        second = submit("second", 4)
        third = submit("third", 4)
        high = submit("high", 5)
        low = submit("low", 1)
        def queue() -> list[dict]:
            return service.call({"op": "decision_queue"})["requests"]
        def ids() -> list[int]:
            return [item["id"] for item in queue()]
        assert ids() == [high, first, second, third, low]
        deferred = {"op": "defer_decision", "id": first, "identity": "defer-first-once"}
        with sqlite3.connect(Path(service.directory.name) / "state.sqlite") as db:
            db.execute("CREATE TRIGGER reject_defer BEFORE INSERT ON decision_events BEGIN SELECT RAISE(ABORT, 'event failure'); END")
        assert service.run(deferred).returncode != 0
        assert ids() == [high, first, second, third, low]
        assert service.call({"op": "decision_events", "agent": "first"})["events"] == []
        with sqlite3.connect(Path(service.directory.name) / "state.sqlite") as db:
            db.execute("DROP TRIGGER reject_defer")
        result = service.call(deferred)
        assert result["id"] == first and result["event_id"] > 0
        assert ids() == [high, second, third, first, low]
        assert service.call(deferred) == result
        assert ids() == [high, second, third, first, low]
        assert service.call({"op": "get", "id": first})["answer"] is None
        events = service.call({"op": "decision_events", "agent": "first"})["events"]
        assert events == [{"id": result["event_id"], "request_id": first, "kind": "deferred"}]
        assert service.call({"op": "decision_events", "agent": "second"})["events"] == []
        notice = subprocess.run([str(BINARY), "wait", service.address, str(first)], capture_output=True, text=True, timeout=8, check=True)
        assert json.loads(notice.stdout) == {"id": first, "answer": None, "deferred_event": result["event_id"]}
        waiter = subprocess.Popen([str(BINARY), "wait", service.address, str(first), "--after-event", str(result["event_id"])], stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        try:
            time.sleep(0.7)
            assert waiter.poll() is None, "consumed deferral was delivered twice"
            service.call({"op": "answer_custom", "id": first, "value": "continue"})
            stdout, stderr = waiter.communicate(timeout=8)
            assert waiter.returncode == 0, stderr
            assert json.loads(stdout)["answer"]["value"] == "continue"
        finally:
            if waiter.poll() is None:
                waiter.kill()
                waiter.communicate()
        # A second request preserves the remaining ordering and restart tests.
        first = submit("first-again", 4)
        result = service.call({"op": "defer_decision", "id": first, "identity": "defer-first-again"})
        deferred = {"op": "defer_decision", "id": first, "identity": "defer-first-again"}
        events = service.call({"op": "decision_events", "agent": "first-again"})["events"]
        assert service.run({**deferred, "id": second}).returncode != 0
        entries = {item["id"]: item for item in queue()}
        swap = {"op": "swap_decisions", "first": second, "second": first,
                "first_version": entries[second]["version"], "second_version": entries[first]["version"],
                "identity": "swap-once"}
        service.call(swap)
        assert ids() == [high, first, third, second, low]
        service.call(swap)
        assert ids() == [high, first, third, second, low]
        stale = {**swap, "identity": "stale-swap"}
        assert "queue changed" in service.run(stale).stderr
        entries = {item["id"]: item for item in queue()}
        service.call({"op": "set_importance", "id": first, "importance": 4})
        stale = {"op": "swap_decisions", "first": first, "second": third,
                 "first_version": entries[first]["version"], "second_version": entries[third]["version"],
                 "identity": "stale-priority"}
        assert "queue changed" in service.run(stale).stderr
        stop(service)
        restart(service)
        assert ids() == [high, first, third, second, low]
        assert service.call(deferred) == result
        assert service.call({"op": "decision_events", "agent": "first-again"})["events"] == events
    with Service() as legacy:
        old = {"identity": "legacy-deferred", "agent": "old", "question": "Old", "options": [], "importance": 3}
        first = legacy.call({"op": "submit", "request": old})["id"]
        second = legacy.call({"op": "submit", "request": {**old, "identity": "legacy-active", "question": "New"}})["id"]
        legacy.call({"op": "set_deferred", "id": first, "deferred": True})
        stop(legacy)
        with sqlite3.connect(Path(legacy.directory.name) / "state.sqlite") as db:
            db.execute("DROP TABLE decision_events")
            db.execute("DROP TABLE decision_queue_positions")
            db.execute("DROP TABLE decision_queue_clock")
        restart(legacy)
        assert [item["id"] for item in legacy.call({"op": "decision_queue"})["requests"]] == [second, first]
        assert legacy.call({"op": "submit", "request": old})["id"] == first
    print("Durable FIFO queue, deferred events, safe retries, stale swaps, and legacy migration passed")


if __name__ == "__main__":
    main()
