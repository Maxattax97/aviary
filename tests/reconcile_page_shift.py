#!/usr/bin/env python3
"""Show why a queue traversal must restart after a change hint."""
import json
import subprocess

from support import BINARY, Service
from zeromq_interop import Dealer


def main():
    with Service() as service:
        ids = []
        for index in range(6):
            operation = {"op": "submit", "request": {
                "identity": f"page-shift-{index}", "agent": "agent",
                "question": f"page-shift-{index}", "options": [],
                "context": str(index) * 350000,
            }}
            result = subprocess.run(
                [str(BINARY), "call", service.address, "-"], input=json.dumps(operation),
                text=True, capture_output=True, timeout=15, check=True,
            )
            ids.append(json.loads(result.stdout)["id"])

        subscriber = Dealer(service.address)
        try:
            subscriber.send("subscribe", {"op": "subscribe_changes"}, version=2)
            assert subscriber.receive(version=2)["result"]["watching"] is True
            first = service.call({"op": "decision_queue", "offset": 0})
            first_ids = [item["id"] for item in first["requests"]]
            offset = first["next_offset"]
            assert offset is not None and 0 < offset < len(ids)
            assert first_ids == ids[:offset]

            service.call({"op": "answer_custom", "id": ids[0], "value": "done"})
            hint = subscriber.receive(version=2)
            assert hint["correlation"] == "event" and hint["result"]["changed"] is True
            shifted = service.call({"op": "decision_queue", "offset": offset})
            shifted_ids = [item["id"] for item in shifted["requests"]]
            # The first item beyond the old boundary moved left. The old
            # offset misses it even though a hint reported the mutation.
            assert ids[offset] not in first_ids + shifted_ids

            reread = []
            offset = 0
            while True:
                page = service.call({"op": "decision_queue", "offset": offset})
                reread.extend(item["id"] for item in page["requests"])
                next_offset = page["next_offset"]
                if next_offset is None:
                    break
                assert next_offset > offset
                offset = next_offset
            assert reread == ids[1:]
        finally:
            subscriber.close()
    print("Queue page shift observed; restart after hint recovered stable state")


if __name__ == "__main__":
    main()
