# Aviary: first-slice prototype

This is an incomplete vertical slice, not V1. It submits a decision, presents its recommended answer, and retrieves a durable correlated response. It does not yet implement channels, general message delivery, custom answers, selection of other answers in the interface, or the full V1 requirements.

The service has **no authentication or encryption**. Any process that can connect can read requests and submit answers. The default listener is `127.0.0.1:7432`; bind a non-loopback address only on a trusted network. Participant names are labels, not identities.

Build with `cargo build --locked`. Start `aviary serve [database-path] [bind-address]`; defaults are `aviary.sqlite` and `127.0.0.1:7432`. Keep the service running independently of clients. For a trusted network explicitly specify, for example, `aviary serve aviary.sqlite 192.168.1.10:7432` using an address actually assigned to the host.

Submit (indices are zero-based):

```sh
aviary call 127.0.0.1:7432 '{"op":"submit","request":{"identity":"unique-retry-key","agent":"agent-1","question":"Proceed?","options":["No","Yes"],"recommendation":1,"reason":"Checks passed"}}'
aviary tui 127.0.0.1:7432
aviary wait 127.0.0.1:7432 1
```

The submission returns a JSON `id`; retain it for `wait` or `aviary call ADDRESS '{"op":"get","id":1}'`. `wait` polls until an answer arrives and can be restarted with the same ID. The interface polls for requests, shows the oldest pending request, and Enter explicitly confirms its recommendation. `q` followed by Enter exits. Other choices are visible but cannot yet be selected. A second answer is rejected. Identical submissions with the same identity return the original ID; differing content is rejected. Caller-supplied identities are deployment-wide and must be unique for different requests.

The protocol is one UTF-8 JSON object per TCP connection, terminated by a newline, with one newline-terminated JSON response. Operations are `submit`, `list`, `get`, and `answer`; errors produce `{"error":"..."}`. The `answer` operation takes `id` and the original zero-based option `index`; a returned answer has `type`, `index`, and `value`. `call` prints successful JSON and exits nonzero on errors. Messages are limited to 1 MiB including the newline. This prototype protocol is not yet a stable public contract.

SQLite WAL with `synchronous=FULL` commits before acknowledgment. This targets process or OS crashes with intact storage and a functioning filesystem; storage loss and backup recovery are not covered. Do not remove the WAL while the database is in use. The database contains private request data; protect the database and its directory.

Run `cargo test --locked`. The integration test needs permission to bind a loopback TCP socket. It kills and restarts the service after acknowledged writes; it does not prove power-loss recovery or cross-machine operation.
