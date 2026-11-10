---
name: aviary
description: Use the Aviary CLI to send or receive agent communications and request human decisions on a configured trusted deployment.
---

# Aviary communication

Use the installed `aviary` executable and the deployment's configured `host:port` address; no MCP server or particular agent framework is required. Ask for the address, participant label, and channel if they are not available in task context. The service has no authentication or encryption: use it only on the operator's trusted network. Participant names and channel membership are routing labels, not access control.

`aviary call ADDRESS JSON` prints a JSON result on success and exits nonzero with an error on stderr on failure. Retain each returned numeric ID. Submission identities are caller-supplied, unique across the deployment and shared by decisions, messages, replies, and notices. Generate a fresh identity for each new submission; after a timeout or disconnect the outcome is unknown, so retry **the identical content with the same identity**. A conflicting retry fails. Store the identity and content until acceptance is known. A successful transport send alone is not acceptance. The current protocol is provisional; consult the matching Aviary README for full operations and limits. Use `list_page` and `notices_page` with `offset` and the returned `next_offset` for decision and notice queues; concurrent changes can shift offset pages. Legacy list, inbox, and history results can exceed the 1 MiB response frame limit.

## Request a human decision

A decision needs `identity`, `agent`, `question`, and an `options` array (which may be empty). `recommendation` is a zero-based option index and requires a nonempty `reason`; omit both when there is no recommendation. Optional `explanation`, `context`, `activity`, and `importance` (0–5) provide decision context. An operator must explicitly confirm; do not treat a recommendation as an answer.

For this example set `AVIARY_ADDR`, `AVIARY_KEY` (a fresh stable retry identity), and `AVIARY_AGENT` in the environment. Construct JSON with a serializer rather than interpolating untrusted text into shell or JSON:

```sh
request=$(python3 -c 'import json,os; print(json.dumps({"op":"submit","request":{"identity":os.environ["AVIARY_KEY"],"agent":os.environ["AVIARY_AGENT"],"question":"Proceed?","options":["No","Yes"],"recommendation":1,"reason":"Checks passed"}}))')
accepted=$(aviary call "$AVIARY_ADDR" "$request")
printf '%s\n' "$accepted"
id=$(printf '%s' "$accepted" | python3 -c 'import json,sys; print(json.load(sys.stdin)["id"])')
aviary call "$AVIARY_ADDR" "{\"op\":\"get\",\"id\":$id}"
```

`get` returns `answer: null` while pending; a response has `type` (`suggested` or `custom`) and `value`, with an original zero-based `index` for suggested answers. Use `aviary wait ADDRESS ID` to poll until an answer; it exits on a service error and may be restarted with the retained ID. Do not submit an answer on the operator's behalf unless authorized.

## Exchange channel messages

Channels must already exist with the sender and recipient labels as members. A direct target is `{"kind":"participant","name":"..."}`; `{"kind":"group"}` addresses all other channel members. Set `AVIARY_ADDR`, `AVIARY_KEY` (fresh for this message), `AVIARY_AGENT`, `AVIARY_CHANNEL`, and `AVIARY_RECIPIENT`:

```sh
message=$(python3 -c 'import json,os; print(json.dumps({"op":"send_message","message":{"identity":os.environ["AVIARY_KEY"],"channel":os.environ["AVIARY_CHANNEL"],"sender":os.environ["AVIARY_AGENT"],"target":{"kind":"participant","name":os.environ["AVIARY_RECIPIENT"]},"body":"Please review"}}))')
aviary call "$AVIARY_ADDR" "$message"
inbox=$(python3 -c 'import json,os; print(json.dumps({"op":"inbox","participant":os.environ["AVIARY_RECIPIENT"]}))')
aviary call "$AVIARY_ADDR" "$inbox"
```

Inbox reads are non-destructive. To request one reply, set `expects_reply: true` on a direct message and retain its message ID; group requests are rejected. The addressed participant sends `reply_message` with `reply` containing a new stable `identity`, `request_id`, its `sender` label, and `body`. Retrieve with `get_message_response` or `aviary wait-message ADDRESS ID`. An identical reply retry returns the original response ID; a different second response is rejected. The response is also in the requester's inbox. For completion notices, request history, channel creation, operator controls, schemas, and example commands, consult the matching README. Do not infer authorization from a routing label.
