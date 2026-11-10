# Aviary: first-slice prototype

See the [system architecture](docs/architecture.md) for the V1 design, boundaries and verification map.

This is an incomplete vertical slice, not V1. It submits a decision, presents its recommended answer, and retrieves a durable correlated response. It does not yet implement operator instruction composition, the full attention interface, or the full V1 requirements.

The service has **no authentication or encryption**. Any process that can connect can read requests and submit answers. The default listener is `127.0.0.1:7432`; bind a non-loopback address only on a trusted network. Participant names are labels, not identities.

Build with `cargo build --locked`. Start `aviary serve [database-path] [bind-address]`; defaults are `aviary.sqlite` and `127.0.0.1:7432`. Keep the service running independently of clients. For a trusted network explicitly specify, for example, `aviary serve aviary.sqlite 192.168.1.10:7432` using an address actually assigned to the host.

Submit (indices are zero-based):

```sh
aviary call 127.0.0.1:7432 '{"op":"submit","request":{"identity":"unique-retry-key","agent":"agent-1","question":"Proceed?","options":["No","Yes"],"recommendation":1,"reason":"Checks passed"}}'
aviary tui 127.0.0.1:7432
aviary wait 127.0.0.1:7432 1
```

The submission returns a JSON `id`; retain it for `wait` or `aviary call ADDRESS '{"op":"get","id":1}'`. `wait` polls until an answer arrives and can be restarted with the same ID. The interface polls for pending requests and orders them by importance, highest first, with submission order for ties. A recommendation is displayed first and preselected; Enter explicitly confirms it. Use h/l or left/right arrows to navigate requests, j/k or up/down arrows to select an answer (or 1–9 for the first nine), Enter to confirm, and q to quit. Press d to defer or restore, Tab to switch active and deferred queues, p followed by 0–5 to override importance, and c to view explanation and supporting context. The last choice opens a custom-answer editor; Enter submits its nonempty text and Escape cancels editing. Without a recommendation no choice is initially selected. A second answer is rejected. Identical submissions with the same identity return the original ID; differing content is rejected. Caller-supplied identities are deployment-wide and must be unique for different requests.

The transport is ZeroMQ over TCP using a DEALER client and ROUTER service. Update clients and service together; the earlier raw-TCP protocol is incompatible. Each DEALER sends one JSON frame containing `version` (1), `correlation` (a nonempty string up to 128 bytes), and `operation`; the service replies with one JSON frame containing the same `version` and `correlation` and a `result` object. The ROUTER identity is transient routing metadata, not a participant identity. A submission accepts an `options` array of zero or more nonempty strings. `recommendation` and `reason` are optional together; when present the zero-based index must identify an option and the reason must be nonempty. Operations are `submit`, `list`, `get`, `answer`, and `answer_custom`; errors produce `{"error":"..."}`. The `answer` operation takes `id` and the original zero-based option `index`; a suggested answer has `type`, `index`, and `value`. `answer_custom` takes `id` and a nonempty `value`; the stored response has `type: "custom"` and the exact `value`. `call` prints successful JSON and exits nonzero on errors. A JSON frame is limited to 1 MiB. Socket queues have a high-water mark of 64 messages per peer; sends and receives time out after 10 seconds and sockets discard queued messages on close. A timeout leaves the outcome unknown: retry a submission with the same identity and content. The transport-level limit is per frame, not a total memory bound on multipart input or connected peers. This prototype protocol is not yet a stable public contract.

The Tokio service uses SQLx with SQLite WAL and `synchronous=FULL`; writes commit before acknowledgment. Service logs go to stderr through tracing; set `RUST_LOG` to adjust their verbosity. This targets process or OS crashes with intact storage and a functioning filesystem; storage loss and backup recovery are not covered. Do not remove the WAL while the database is in use. The database contains private request data; protect the database and its directory.

Run `cargo test --locked`. Building requires a C++ compiler and CMake for the bundled libzmq when a system development package is unavailable. The integration test needs permission to bind a loopback TCP socket. It kills and restarts the service after acknowledged writes; it does not prove power-loss recovery or cross-machine operation.

For a Linux pseudo-terminal smoke test after building, run `python3 tests/terminal_pty.py`; it requires loopback socket access and exercises recommendation, alternate and custom answers and terminal restoration.

### Channel messages (initial CLI contract)

Create an immutable named channel with a nonempty, unique list of participant labels. Repeating its name with the same membership succeeds; a different membership conflicts. A sender must be a member. A participant target must be a member; a group target addresses all members except the sender and must have at least one recipient. Recipient membership is captured at acceptance. Inbox retrieval is non-destructive and returns retained messages in service ID order, including messages accepted while a recipient was disconnected. Names are routing labels and anyone who can reach the service can impersonate them or read any inbox.

```sh
aviary call 127.0.0.1:7432 '{"op":"create_channel","channel":{"name":"team","members":["agent-a","agent-b"]}}'
aviary call 127.0.0.1:7432 '{"op":"send_message","message":{"identity":"unique-message-key","channel":"team","sender":"agent-a","target":{"kind":"participant","name":"agent-b"},"body":"Please review","context":"work description","activity":"task-1"}}'
aviary call 127.0.0.1:7432 '{"op":"inbox","participant":"agent-b"}'
```

Use `{"kind":"group"}` as the target for channel-wide delivery. `context` and `activity` may be omitted. Message and decision submission identities share one deployment-wide namespace; an identical retry returns the original ID, while changed content or a different submission type conflicts. Message and decision IDs themselves are separate. The inbox is not yet paginated and can exceed the transport response limit.

### Agent request and reply

Set `"expects_reply":true` on a `send_message` addressed to one participant; group requests are rejected. The recipient sees the request in its inbox. Retain the returned message ID and use `aviary wait-message ADDRESS ID`, or call `{"op":"get_message_response","id":ID}` to retrieve a response later. Submit the response as `{"op":"reply_message","reply":{"identity":"unique-reply-key","request_id":ID,"sender":"agent-b","body":"Done"}}`. The response is also routed as a message to the requester's inbox and carries the request's context and activity. The named sender must match the original target label; this is a routing check, not authentication. One response is accepted; an identical reply retry with the same identity returns its original message ID, and a different response after acceptance is rejected. A response retry key shares the deployment-wide submission namespace. The waiter polls until a response or service error and can be restarted with the same ID.

Decision submissions may include `"importance":0` through `5` (default 0); higher is more important. The operator override persists separately from the submission and survives restart. `set_importance` takes `id` and `importance`; `set_deferred` takes `id` and a boolean `deferred`; `list_deferred` retrieves deferred pending requests. Deferral is not an answer and has no automatic resurfacing. Queue responses are not yet paginated and can exceed the transport response limit.

Decision requests may include optional `explanation` and `context` strings. The queue shows up to 160 characters of explanation; c opens the complete explanation and context in 200-character pages, navigated with j/k and closed with c, q or Escape. Terminal control characters, including newlines, are displayed as spaces; stored content is unchanged. The existing 1 MiB frame limit applies, and queue pagination and terminal-size-aware layout remain open.
