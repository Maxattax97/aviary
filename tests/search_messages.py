#!/usr/bin/env python3
"""Exercise bounded retained-message search and cursor semantics."""
from support import Service


def main() -> None:
    with Service() as service:
        service.call({"op": "create_channel", "channel": {"name": "search", "members": ["alpha", "operator"]}})
        ids = []
        for n in range(27):
            ids.append(service.call({"op": "send_message", "message": {
                "identity": f"search-{n}", "channel": "search", "sender": "alpha",
                "target": {"kind": "participant", "name": "operator"},
                "body": "Long Needle " + "x" * 500, "context": "Launch context", "activity": "work-42"
            }})["id"])
        service.call({"op": "send_message", "message": {"identity": "other", "channel": "search", "sender": "alpha", "target": {"kind": "participant", "name": "operator"}, "body": "Unrelated"}})
        first = service.call({"op": "search_messages", "query": "NEEDLE"})
        assert [item["id"] for item in first["messages"]] == ids[:25]
        assert all(len(item["body"]) == 160 for item in first["messages"])
        assert first["next_after"] == ids[24]
        second = service.call({"op": "search_messages", "query": "needle", "after": first["next_after"]})
        assert [item["id"] for item in second["messages"]] == ids[25:]
        assert second["next_after"] is None
        for query in ["ALPHA", "launch context", "work-42"]:
            assert service.call({"op": "search_messages", "query": query})["messages"]
        assert service.call({"op": "search_messages", "query": "%"})["messages"] == []
        assert service.call({"op": "search_messages", "query": "missing"})["messages"] == []
        for query, after in [(" ", 0), ("x" * 257, 0), ("x", -1)]:
            assert service.run({"op": "search_messages", "query": query, "after": after}).returncode != 0
        history = service.call({"op": "history_page", "kind": "message", "id": ids[0], "section": "messages"})
        assert history["messages"][0]["id"] == ids[0]
        root_first = service.call({"op": "history_page", "kind": "message", "id": ids[-1], "section": "messages", "root_first": True})
        assert root_first["messages"][0]["id"] == ids[-1]
    print("Retained message search, literal matching, bounded previews and cursor paging passed")


if __name__ == "__main__":
    main()
