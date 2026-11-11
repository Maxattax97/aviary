#!/usr/bin/env python3
"""Recover multiple missed changes after subscription loss and restart."""
import json
import subprocess

from support import BINARY, Service
from terminal_reconnect import restart, stop
from zeromq_interop import Dealer


def main():
    with Service() as service:
        def call_large(operation):
            result = subprocess.run([str(BINARY), "call", service.address, "-"], input=json.dumps(operation),
                                    capture_output=True, text=True, timeout=15, check=True)
            return json.loads(result.stdout)
        def decision(identity, context=""):
            return call_large({"op": "submit", "request": {"identity": identity, "agent": "agent",
                "question": identity, "context": context, "options": [], "importance": 4}})["id"]
        service.call({"op": "create_channel", "channel": {"name": "reconcile", "members": ["agent", "operator"]}})
        deferred_id = decision("before-deferred")
        answered_id = decision("before-answered")
        first_defer = service.call({"op": "defer_decision", "id": deferred_id, "identity": "before-event"})
        cursor = first_defer["event_id"]
        subscription = Dealer(service.address)
        subscription.send("subscribe", {"op": "subscribe_changes"}, version=2)
        assert subscription.receive(version=2)["result"]["watching"] is True
        subscription.close()
        later_defer = {"op": "defer_decision", "id": deferred_id, "identity": "while-away"}
        later_event = service.call(later_defer)
        assert service.call(later_defer) == later_event
        service.call({"op": "answer_custom", "id": answered_id, "value": "finished while away"})
        contexts = [str(i) * 350000 for i in range(3)]
        created = [decision(f"offline-decision-{i}", context) for i, context in enumerate(contexts)]
        message_ids = []
        for i, body in enumerate(contexts):
            message_ids.append(call_large({"op": "send_message", "message": {"identity": f"offline-message-{i}",
                "channel": "reconcile", "sender": "agent", "target": {"kind": "participant", "name": "operator"},
                "body": body}})["id"])
        stop(service)
        restart(service)
        subscription = Dealer(service.address)
        try:
            subscription.send("subscribe", {"op": "subscribe_changes"}, version=2)
            assert subscription.receive(version=2)["result"]["watching"] is True
            # A new subscription is a trigger to reread, not evidence that any
            # particular request was accepted or that missed hints were replayed.
            offset = 0
            decisions = []
            decision_pages = 0
            while True:
                page = service.call({"op": "decision_queue", "offset": offset})
                decisions.extend(page["requests"])
                decision_pages += 1
                if page["next_offset"] is None:
                    break
                assert page["next_offset"] > offset
                offset = page["next_offset"]
            assert decision_pages > 1
            assert {item["id"] for item in decisions} == {deferred_id, *created}
            assert answered_id not in [item["id"] for item in decisions]
            assert service.call({"op": "get", "id": answered_id})["answer"]["value"] == "finished while away"
            assert [item["request"]["context"] for item in decisions if item["id"] in created] == contexts
            events = service.call({"op": "decision_events", "agent": "agent", "after": cursor})
            assert events["events"] == [{"id": later_event["event_id"], "request_id": deferred_id, "kind": "deferred"}]
            assert events["next_after"] is None
            offset = 0
            messages = []
            message_pages = 0
            while True:
                page = service.call({"op": "inbox_page", "participant": "operator", "offset": offset})
                messages.extend(page["messages"])
                message_pages += 1
                if page["next_offset"] is None:
                    break
                assert page["next_offset"] > offset
                offset = page["next_offset"]
            assert message_pages > 1
            assert [item["id"] for item in messages] == message_ids
            assert [item["message"]["body"] for item in messages] == contexts
            assert service.call(later_defer) == later_event
            assert service.call(later_defer) == later_event
            for _ in range(2):
                hint = subscription.receive(version=2)
                assert hint["correlation"] == "event" and hint["result"]["changed"] is True
            assert service.call({"op": "decision_events", "agent": "agent", "after": cursor}) == events
        finally:
            subscription.close()
    print("Reconnected after missed decisions, deferral, answer and messages; bounded reconciliation passed")


if __name__ == "__main__":
    main()
