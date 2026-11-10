# Aviary: first-slice prototype

See the [system architecture](docs/architecture.md) for the V1 design, boundaries and verification map.

This is an incomplete vertical slice, not V1. It submits a decision, presents its recommended answer, and retrieves a durable correlated response. It does not yet implement channels, general message delivery, the full attention interface, or the full V1 requirements.

The service has **no authentication or encryption**. Any process that can connect can read requests and submit answers. The default listener is `127.0.0.1:7432`; bind a non-loopback address only on a trusted network. Participant names are labels, not identities.

Build with `cargo build --locked`. Start `aviary serve [database-path] [bind-address]`; defaults are `aviary.sqlite` and `127.0.0.1:7432`. Keep the service running independently of clients. For a trusted network explicitly specify, for example, `aviary serve aviary.sqlite 192.168.1.10:7432` using an address actually assigned to the host.

Submit (indices are zero-based):

```sh
aviary call 127.0.0.1:7432 '{"op":"submit","request":{"identity":"unique-retry-key","agent":"agent-1","question":"Proceed?","options":["No","Yes"],"recommendation":1,"reason":"Checks passed"}}'
aviary tui 127.0.0.1:7432
aviary wait 127.0.0.1:7432 1
```

The submission returns a JSON `id`; retain it for `wait` or `aviary call ADDRESS '{"op":"get","id":1}'`. `wait` polls until an answer arrives and can be restarted with the same ID. The interface polls for requests and shows the oldest pending request. A recommendation is displayed first and preselected; Enter explicitly confirms it. Use j/k or arrow keys to select a choice (or 1–9 for the first nine), Enter to confirm, and q to quit. The last choice opens a custom-answer editor; Enter submits its nonempty text and Escape cancels editing. Without a recommendation no choice is initially selected. A second answer is rejected. Identical submissions with the same identity return the original ID; differing content is rejected. Caller-supplied identities are deployment-wide and must be unique for different requests.

The transport is ZeroMQ over TCP using a DEALER client and ROUTER service. Update clients and service together; the earlier raw-TCP protocol is incompatible. Each DEALER sends one JSON frame containing `version` (1), `correlation` (a nonempty string up to 128 bytes), and `operation`; the service replies with one JSON frame containing the same `version` and `correlation` and a `result` object. The ROUTER identity is transient routing metadata, not a participant identity. A submission accepts an `options` array of zero or more nonempty strings. `recommendation` and `reason` are optional together; when present the zero-based index must identify an option and the reason must be nonempty. Operations are `submit`, `list`, `get`, `answer`, and `answer_custom`; errors produce `{"error":"..."}`. The `answer` operation takes `id` and the original zero-based option `index`; a suggested answer has `type`, `index`, and `value`. `answer_custom` takes `id` and a nonempty `value`; the stored response has `type: "custom"` and the exact `value`. `call` prints successful JSON and exits nonzero on errors. A JSON frame is limited to 1 MiB. Socket queues have a high-water mark of 64 messages per peer; sends and receives time out after 10 seconds and sockets discard queued messages on close. A timeout leaves the outcome unknown: retry a submission with the same identity and content. The transport-level limit is per frame, not a total memory bound on multipart input or connected peers. This prototype protocol is not yet a stable public contract.

The Tokio service uses SQLx with SQLite WAL and `synchronous=FULL`; writes commit before acknowledgment. Service logs go to stderr through tracing; set `RUST_LOG` to adjust their verbosity. This targets process or OS crashes with intact storage and a functioning filesystem; storage loss and backup recovery are not covered. Do not remove the WAL while the database is in use. The database contains private request data; protect the database and its directory.

Run `cargo test --locked`. Building requires a C++ compiler and CMake for the bundled libzmq when a system development package is unavailable. The integration test needs permission to bind a loopback TCP socket. It kills and restarts the service after acknowledged writes; it does not prove power-loss recovery or cross-machine operation.

For a Linux pseudo-terminal smoke test after building, run `python3 tests/terminal_pty.py`; it requires loopback socket access and exercises recommendation, alternate and custom answers and terminal restoration.
