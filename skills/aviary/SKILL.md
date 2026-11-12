---
name: aviary
description: Use Aviary CLI for agent communications and human decisions on a configured trusted deployment.
---

# Aviary

Use the installed `aviary` CLI and configured `host:port` address. The CLI accepts one JSON operation with `aviary call ADDRESS JSON`, prints JSON on success, and exits nonzero on error. See Aviary's `docs/cli.md` for all operations and current schemas.

Aviary separates three kinds of work:

- **Decisions** are actionable questions for the operator. Submit one only when human judgment or authorization is needed. The operator's answer is explicit; a recommendation is not an answer.
- **Messages** are targeted communications between participants or groups. They appear in the recipient's inbox, not the decision queue.
- **Notices** report completed work and its result. They appear in the operator's completion notices, not as a request for a decision.

Pending decisions have importance 0–5 (default 0). Higher importance appears first; items of equal importance retain their queue order. The operator can change importance, and deferral keeps a decision pending while moving it to the bottom of its importance group. Do not use a decision as a general status message.

## Submit and retrieve a decision

Give every new submission a fresh deployment-wide `identity`; retain it with the exact content. If the result is unknown after timeout or disconnect, retry the same operation with the same identity and content. An identical retry returns the original ID; changed content with that identity fails. Set `AVIARY_ADDR`, `AVIARY_KEY`, and `AVIARY_AGENT` for these examples.

```sh
request=$(python3 -c 'import json,os; print(json.dumps({"op":"submit","request":{"identity":os.environ["AVIARY_KEY"],"agent":os.environ["AVIARY_AGENT"],"question":"Proceed?","options":["No","Yes"],"importance":4,"recommendation":1,"reason":"Checks passed"}}))')
accepted=$(aviary call "$AVIARY_ADDR" "$request")
printf '%s\n' "$accepted"
id=$(printf '%s' "$accepted" | python3 -c 'import json,sys; print(json.load(sys.stdin)["id"])')
aviary call "$AVIARY_ADDR" "{\"op\":\"get\",\"id\":$id}"
```

`get` returns `answer: null` while pending. Use `aviary wait ADDRESS ID` to wait for an answer or deferral; a deferral is not an answer. Do not answer on the operator's behalf unless explicitly authorized.

## Send and retrieve a message

The channel must already include the sender and recipient. Set `AVIARY_CHANNEL` and `AVIARY_RECIPIENT` too. Inbox reads do not remove messages.

```sh
message=$(python3 -c 'import json,os; print(json.dumps({"op":"send_message","message":{"identity":os.environ["AVIARY_KEY"],"channel":os.environ["AVIARY_CHANNEL"],"sender":os.environ["AVIARY_AGENT"],"target":{"kind":"participant","name":os.environ["AVIARY_RECIPIENT"]},"body":"Please review"}}))')
aviary call "$AVIARY_ADDR" "$message"
inbox=$(python3 -c 'import json,os; print(json.dumps({"op":"inbox_page","participant":os.environ["AVIARY_RECIPIENT"],"offset":0}))')
aviary call "$AVIARY_ADDR" "$inbox"
```

Use `inbox_page` and `notices_page` with `offset`, following `next_offset` until null. Submit completed work with `submit_notice`; use `decision_queue` to read pending decisions. For larger operations, pass JSON on stdin with `aviary call ADDRESS - < operation.json`.

The service has no authentication, authorization, or encryption. Participant and channel names route communications; they do not establish identity or access control. Use only on the configured trusted deployment. The protocol is provisional; coordinate client updates with the service and current CLI documentation.
