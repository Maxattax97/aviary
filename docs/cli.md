# Aviary CLI and operation reference

This is the **provisional** contract for the Aviary version in this repository. Deploy the service, CLI and portable skill together; a stable compatibility policy has not yet been established. The service is for a trusted single-user network: it does not authenticate, authorize, or encrypt traffic. Anyone who can connect can impersonate a label and access or alter communications.

## Invocation and results

`aviary serve [DB] [BIND]` starts the service, defaulting to `aviary.sqlite` and `127.0.0.1:7432`. `aviary tui` starts or reuses a local service at `127.0.0.1:7432` using `aviary.sqlite` in the current directory; `aviary tui --local-address 127.0.0.1:PORT` selects another IPv4 loopback endpoint. Both verify service protocol and canonical database path. An explicit positional `aviary tui ADDRESS` only connects and never starts a service. The local child continues after TUI exit and logs to `aviary-service.log` in the current directory. A startup timeout is an unknown outcome: the child may still start. `aviary call ADDRESS JSON` sends one operation; ADDRESS is `host:port`, and JSON is one serialized object. Use `aviary call ADDRESS - < operation.json` to read one JSON operation from standard input until EOF; this avoids operating-system command-line argument limits. Input is limited to 1 MiB including whitespace, and the serialized wire envelope must also fit the request frame limit. Malformed or oversized input exits nonzero before sending. `aviary wait ADDRESS ID` waits for a decision answer or deferral event, while `aviary wait-message ADDRESS ID` waits for a direct message response. Both subscribe to change hints and reread durable state; they exit on a connection or service error. Restart with the retained ID and, for a consumed deferral, its event cursor after an interruption.

Successful `call` and completed `wait` commands write one JSON object to stdout and exit zero. Errors produce a nonzero exit and diagnostic text on stderr; error strings are not a stable machine-readable error-code API. The CLI uses a version-2 ZeroMQ envelope with a 1 MiB request limit and a 3 MiB response limit, including each envelope. An operation or response can exceed its limit even if an individual field is smaller. Upgrade the service before version-2 clients; version-1 clients remain supported with a 1 MiB response limit. `call` has a ten-second send and receive timeout; an interrupted or timed-out operation may already have committed.

Decision, message, reply, and notice submissions each require a caller-supplied, deployment-wide unique `identity`. Retain the identity and exact content before sending. For an unknown outcome, retry the same operation and content with the same identity: the service returns its original ID for an identical retry and rejects a conflicting reuse. A successful application response follows the SQLite commit. This identity mechanism does **not** apply to operator answers: a second answer is rejected, so retrieve the request's answer before deciding what to do after an uncertain answer. IDs for different record kinds can overlap; use the right operation and history root kind.

## ZeroMQ wire envelope

The CLI is the recommended integration boundary. A direct client compatible with this provisional version connects a DEALER socket to `tcp://HOST:PORT` and sends exactly one UTF-8 JSON frame per operation:

```json
{"version":2,"correlation":"client-chosen-token","operation":{"op":"list_page","offset":0}}
```

The service ROUTER supplies its routing identity as transport metadata; the application does not place that identity in its JSON. The service responds to that connection with one JSON frame of the form `{"version":2,"correlation":"client-chosen-token","result":{...}}`. Version 2 permits responses up to 3 MiB; version 1 remains supported with a 1 MiB response limit and receives a version-1 envelope. Requests for both versions remain limited to 1 MiB. Configure direct clients for the response size they negotiate. A service error is represented inside `result` as `{"error":"..."}`. Use a nonempty correlation string of at most 128 UTF-8 bytes and match replies by correlation when multiple operations are outstanding; do not use it as a durable retry identity. Unsupported versions receive an error, while malformed envelopes or multipart messages may be discarded without a reply. A send, reconnect, or missing reply is not proof of acceptance. The CLI translates the result into its stdout/exit-status contract above. This envelope is provisional and is not a commitment to interoperability with future versions.

`aviary wait ADDRESS ID` returns an answer or a `deferred_event` ID. To continue waiting after handling a deferral, pass `--after-event EVENT_ID`; a final answer is returned in the original answer shape. The waiter subscribes to change hints and rereads durable state after each hint; it exits on a connection or service error.

Direct change subscriptions require a fresh dedicated connection used only for that subscription: multiplexing request/reply traffic can invalidate the full-queue coalescing assumption, and the service does not enforce this restriction. `aviary watch ADDRESS` emits newline-delimited JSON containing `connected` and a process-local `revision`. It reports disconnections and reconnects automatically. A connection event or revision change is a hint to reread the relevant durable queue, event cursor, or inbox; it is not a durable event ID and notifications can coalesce.

## Operation shapes

The table lists the JSON object supplied to `call` and the success result. Strings described as required must be nonempty. An `id` is a numeric record ID returned by the service. Payload structs reject unknown fields; the outer operation envelope is provisional. Fields marked optional may be omitted. A field shown as `null` in a result is JSON null.

| Operation | Input object | Success result and notes |
| --- | --- | --- |
| Service identity | `{"op":"service_info"}` | Returns service name, protocol version, canonical database path and current process ID. This is an operational identity check, not authentication. |
| Overview | `{"op":"overview"}` | Returns `{"decisions":N,"messages":N,"notices":N}` from one database snapshot: unanswered decisions, retained messages addressed to `operator`, and undismissed completion notices. Messages are not an unread count. |
| Submit decision | `{"op":"submit","request":REQUEST}` | `{"id":ID}`; see decision fields below. |
| Get decision | `{"op":"get","id":ID}` | `{"id":ID,"answer":null}` while pending, or an answer object below. |
| List decision queue | `{"op":"decision_queue","offset":0}` | Returns pending requests in descending importance and position order, each with `id`, `request`, `importance`, opaque `position`, and `version`; `next_offset` traverses bounded pages. `newest_id` is the highest submitted decision ID or null. Pages are not one snapshot. |
| Defer decision | `{"op":"defer_decision","id":ID,"identity":"unique-action"}` | Returns `{"id":ID,"event_id":EVENT_ID}` after atomically moving the pending request to the bottom of its importance group and recording a deferral event. Identical retries with the same identity return the same event. |
| Swap decisions | `{"op":"swap_decisions","first":A,"first_version":VA,"second":B,"second_version":VB,"identity":"unique-action"}` | Atomically swaps equal-importance pending decisions if both versions match the queue listing. A stale version or changed priority is rejected. Identical retries retain the result. |
| Decision events | `{"op":"decision_events","agent":"a","after":0}` | Returns retained `events` with increasing IDs, each containing `id`, `request_id`, and `kind`. If `next_after` is non-null, continue from that cursor. Labels route; they do not authenticate. |
| Decision status | `{"op":"decision_status","id":ID,"after":0}` | Returns `id`, current `answer`, and the first `deferred_event` after the cursor, or null. An answer takes precedence when using the CLI waiter. |
| Set priority | `{"op":"set_importance","id":ID,"importance":N}` | `{"id":ID,"importance":N}`; N is 0–5 and the request must be pending. |
| Legacy decision queues | `{"op":"list_page","deferred":false,"offset":0}` | Compatibility-only active/deferred view of the old flag; `list`, `list_deferred`, and `set_deferred` retain old semantics. Use `decision_queue` and `defer_decision` for the single queue and requester events. |
| Answer suggestion | `{"op":"answer","id":ID,"index":N}` | `{"id":ID}`; index is the **original zero-based** option index. A later `get` includes `"answer":{"type":"suggested","index":N,"value":"..."}`. |
| Answer custom | `{"op":"answer_custom","id":ID,"value":"text"}` | `{"id":ID}`; value must be nonempty. A later `get` includes `"answer":{"type":"custom","value":"text"}`. |
| Create channel | `{"op":"create_channel","channel":{"name":"team","members":["a","b"]}}` | `{"channel":{"name":"team","members":["a","b"]}}`. Names and members must be nonempty, members unique. Membership is sorted and immutable; identical creation is idempotent. |
| Send message | `{"op":"send_message","message":MESSAGE}` | `{"id":ID}`; see message fields below. |
| Read inbox | `{"op":"inbox_page","participant":"b","offset":0}` | `{"messages":[{"id":ID,"message":MESSAGE}],"next_offset":N_OR_NULL}`. `inbox` gives the unpaged array. Reads do not remove messages. |
| Search messages | `{"op":"search_messages","query":"text","after":0}` | Returns at most 25 retained messages with `id`, `sender`, `body`, and `context` previews, and `next_after` as a message-ID cursor or null. The query is a literal substring of body, sender, context or activity, with ASCII-only case folding; it must contain non-whitespace and be at most 256 Unicode scalar values. `after` defaults to zero. Pages are separate snapshots; new higher-ID messages can appear on later pages. |
| Reply | `{"op":"reply_message","reply":{"identity":"unique-reply","request_id":ID,"sender":"b","body":"done"}}` | `{"id":REPLY_ID,"request_id":ID}`. Only the addressed label may supply one reply to a direct request. |
| Get reply | `{"op":"get_message_response","id":ID}` | `{"id":ID,"response":null}` while waiting, or `{"id":ID,"response":{"id":REPLY_ID,"message":MESSAGE}}`. |
| Submit notice | `{"op":"submit_notice","notice":{"identity":"unique-notice","agent":"a","work":"build","result":"passed","activity":"task"}}` | `{"id":ID}`. `identity`, `agent`, `work`, and `result` are nonempty; `activity` is optional. |
| Read notices | `{"op":"notices_page","include_dismissed":false,"offset":0}` | `{"notices":[{"id":ID,"notice":NOTICE,"dismissed":false}],"next_offset":N_OR_NULL}`. Inputs default to false/zero; `list_notices` is unpaged. |
| Dismiss notice | `{"op":"dismiss_notice","id":ID}` | `{"id":ID,"dismissed":true}`. The record remains available when dismissed notices are included. |
| Read history | `{"op":"history","kind":"decision","id":ID}` | `{"root":{"kind":"decision","id":ID},"activity":STRING_OR_NULL,"decisions":[...],"messages":[...],"outcomes":[...]}`; kind is `decision` or `message`. |
| Page history | `{"op":"history_page","kind":"message","id":ID,"section":"messages","offset":0}` | `{"id":ID,"section":"messages","messages":[...],"next_offset":N_OR_NULL}`. Section is `decisions`, `messages`, or `outcomes`; traverse each separately. Offset defaults to zero. Optional `root_first: true` puts a message root first in the messages section; remaining records retain ID order. |

A `REQUEST` contains required `identity`, `agent`, `question`, and `options` (an array of nonempty strings, possibly empty). Optional `importance` is 0–5 and defaults to zero. Optional `explanation`, `context`, and `activity` are strings; if supplied, activity cannot be empty. A zero-based `recommendation` and nonempty `reason` must be supplied together, or both omitted. The operator explicitly confirms the selected answer.

A `MESSAGE` contains required `identity`, `channel`, `sender`, `target`, and `body`; the first three and body are nonempty. `target` is `{"kind":"participant","name":"b"}` or `{"kind":"group"}`. The sender and direct target must be channel members. A group snapshots all members except the sender and must have at least one recipient. Optional `context` and `activity` carry caller-supplied text; `expects_reply` defaults to false and can be true only for a direct target. A reply inherits the request's channel, context and activity and appears in the original sender's inbox.

History groups records with the same nonempty activity and also includes direct request/reply links for a message root without activity. Decisions are `{"id":ID,"request":REQUEST,"answer":ANSWER_OR_NULL}`; messages are `{"id":ID,"message":MESSAGE,"reply_to":ID_OR_NULL}`; outcomes are `{"id":ID,"notice":NOTICE,"dismissed":BOOL}`. Each section is ordered by its own record ID, not a global chronology. A page and its root lookup use one read snapshot, but separate pages do not: concurrent changes can shift offsets. Start at offset zero and follow `next_offset` until null. Unpaged operations and records imported from older databases can exceed a response frame. No page traversal or terminal aggregation is a guarantee of bounded total process memory.

## Executable local example

Run the following only against a disposable, trusted deployment. It creates uniquely named records, explicitly answers a decision to demonstrate the machine-readable result, sends a direct agent request and reply, and checks linked history. Replace the example actor actions with your own operator and agents in real use.

```python
import json
import os
import subprocess
import uuid

address = os.environ["AVIARY_ADDR"]
key = "example-" + uuid.uuid4().hex
channel = key + "-channel"
activity = key + "-activity"


def call(operation):
    completed = subprocess.run(
        ["aviary", "call", address, json.dumps(operation)],
        check=True, text=True, capture_output=True,
    )
    return json.loads(completed.stdout)


call({"op": "create_channel", "channel": {"name": channel, "members": ["a", "b"]}})
decision = {"identity": key + "-decision", "agent": "a", "question": "Proceed?", "options": ["No", "Yes"], "recommendation": 1, "reason": "Ready", "activity": activity}
decision_id = call({"op": "submit", "request": decision})["id"]
assert call({"op": "submit", "request": decision})["id"] == decision_id
assert call({"op": "get", "id": decision_id})["answer"] is None
call({"op": "answer", "id": decision_id, "index": 1})
answer = call({"op": "get", "id": decision_id})["answer"]
assert answer == {"type": "suggested", "index": 1, "value": "Yes"}

message = {"identity": key + "-message", "channel": channel, "sender": "a", "target": {"kind": "participant", "name": "b"}, "body": "Check this", "expects_reply": True, "activity": activity}
message_id = call({"op": "send_message", "message": message})["id"]
assert call({"op": "get_message_response", "id": message_id})["response"] is None
reply = {"identity": key + "-reply", "request_id": message_id, "sender": "b", "body": "Checked"}
reply_id = call({"op": "reply_message", "reply": reply})["id"]
assert call({"op": "reply_message", "reply": reply})["id"] == reply_id
assert call({"op": "get_message_response", "id": message_id})["response"]["message"]["body"] == "Checked"
call({"op": "submit_notice", "notice": {"identity": key + "-notice", "agent": "b", "work": "Check", "result": "Done", "activity": activity}})
history = call({"op": "history", "kind": "decision", "id": decision_id})
assert history["activity"] == activity
assert history["messages"][0]["id"] == message_id
assert history["messages"][1]["id"] == reply_id
assert history["outcomes"][0]["notice"]["result"] == "Done"
print(json.dumps({"decision_id": decision_id, "answer": answer, "message_id": message_id, "reply_id": reply_id}))
```
