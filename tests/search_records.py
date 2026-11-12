#!/usr/bin/env python3
"""Search retained decisions, messages and notices across kind cursors."""
from support import Service


def main():
    with Service() as service:
        service.call({"op": "create_channel", "channel": {"name": "search-all", "members": ["agent", "operator"]}})
        decisions = []
        for i in range(24):
            decisions.append(service.call({"op": "submit", "request": {"identity": f"search-decision-{i}",
                "agent": "agent", "question": f"shared-marker decision {i}", "context": "decision context",
                "explanation": "special explanation", "activity": "shared-activity",
                "options": ["special suggestion"], "recommendation": 0, "reason": "special rationale"}})["id"])
        service.call({"op": "answer_custom", "id": decisions[-1], "value": "unusual answer"})
        messages = []
        for i in range(2):
            messages.append(service.call({"op": "send_message", "message": {"identity": f"search-message-{i}",
                "channel": "search-all", "sender": "agent", "target": {"kind": "participant", "name": "operator"},
                "body": f"shared-marker message {i}", "activity": "shared-activity"}})["id"])
        notices = []
        for i in range(2):
            notices.append(service.call({"op": "submit_notice", "notice": {"identity": f"search-notice-{i}",
                "agent": "agent", "work": f"shared-marker notice {i}", "result": "special result", "activity": "shared-activity"}})["id"])
        service.call({"op": "dismiss_notice", "id": notices[-1]})
        page1 = service.call({"op": "search_records", "query": "SHARED-MARKER"})
        assert [(item["kind"], item["id"]) for item in page1["items"]] == [("decision", i) for i in decisions] + [("message", messages[0])]
        assert page1["next_after"] == {"kind": "message", "id": messages[0]}
        page2 = service.call({"op": "search_records", "query": "shared-marker", "after": page1["next_after"]})
        assert [(item["kind"], item["id"]) for item in page2["items"]] == [("message", messages[1])] + [("notice", i) for i in notices]
        assert page2["next_after"] is None
        for query, expected_kind, expected_id in [("unusual answer", "decision", decisions[-1]),
                ("special explanation", "decision", decisions[0]), ("special suggestion", "decision", decisions[0]),
                ("special rationale", "decision", decisions[0]), ("special result", "notice", notices[0])]:
            items = service.call({"op": "search_records", "query": query})["items"]
            assert any(item["kind"] == expected_kind and item["id"] == expected_id for item in items)
        assert service.call({"op": "search_records", "query": "%"})["items"] == []
        assert service.call({"op": "search_records", "query": "missing"})["items"] == []
        assert service.run({"op": "search_records", "query": "agent", "after": {"kind": "notice", "id": 0}}).returncode != 0
        for kind, item_id, section, field in [("decision", decisions[-1], "decisions", "request"),
                ("message", messages[-1], "messages", "message"), ("notice", notices[-1], "outcomes", "notice")]:
            page = service.call({"op": "history_page", "kind": kind, "id": item_id, "section": section, "root_first": True})
            assert page[section][0]["id"] == item_id and field in page[section][0]
        unpaged = service.call({"op": "history", "kind": "notice", "id": notices[-1]})
        assert unpaged["root"] == {"kind": "notice", "id": notices[-1]}
        assert {item["id"] for item in unpaged["outcomes"]} == set(notices)
    print("Search across retained record kinds, cursors and root-first history passed")


if __name__ == "__main__":
    main()
