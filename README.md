<p align="center">
  <img src="assets/aviary.svg" width="112" alt="Aviary logo">
</p>

# Aviary

Aviary is a communication service and terminal attention interface for a human operator and agents working across processes or trusted machines. Agents can request decisions, exchange targeted messages, report completed work, and recover accepted communications after a service restart. The service runs independently of the terminal interface and does not manage agent execution.

**Status:** Aviary is an evolving prototype, not a V1 release. The core workflows below work locally; two-machine, OS-crash, supported-platform and representative-operator acceptance are still open. The protocol and CLI are provisional. See [verification](docs/verification.md) for actual evidence and [tickets](docs/tickets.md) for remaining work.

**Trust boundary:** The service has no authentication, authorization or encryption. Any process that can connect can read communications, impersonate a participant and submit answers. It listens on `127.0.0.1:7432` by default; expose it only to an explicitly trusted network. Participant and channel names are routing labels, not security identities.

## Build and start

A Rust toolchain is required. Building bundled libzmq also requires a C++ compiler and CMake when a suitable system development package is unavailable.

```sh
cargo build --locked
./target/debug/aviary serve
```

The service defaults to `aviary.sqlite` and `127.0.0.1:7432`. To choose both the database and listener:

```sh
./target/debug/aviary serve /path/to/aviary.sqlite 192.168.1.10:7432
```

Use an address assigned to the host and a trusted network. Keep the service running while clients connect; closing the interface does not stop it. Stop the service with Ctrl-C or SIGTERM. It stops taking new work after observing the signal and lets a request already being processed finish; replies may still be lost, so retry unknown outcomes with the same identity. The SQLite database and its WAL can contain private work: protect their directory and do not remove the WAL while the service is running.

## Ask for a decision

In another terminal, submit a request. Choose a new deployment-wide `identity` for each new submission; the recommendation is a zero-based index.

```sh
./target/debug/aviary call 127.0.0.1:7432 '{"op":"submit","request":{"identity":"example-decision-1","agent":"agent-1","question":"Proceed?","options":["No","Yes"],"recommendation":1,"reason":"Checks passed"}}'
./target/debug/aviary tui 127.0.0.1:7432
```

The first command returns a JSON `id`. Replace `1` below with that ID to wait for the answer or retrieve it later:

```sh
./target/debug/aviary wait 127.0.0.1:7432 1
./target/debug/aviary call 127.0.0.1:7432 '{"op":"get","id":1}'
```

The interface presents a recommendation first and preselects it, but Enter is an explicit decision. Requests can have no suggested answers; the operator can select another suggestion or enter a custom answer. While editing a custom answer, Ctrl-R opens its paged review; q or Esc returns to editing, and Enter submits. The response distinguishes suggested and custom answers. A second decision is rejected.

| Key | Action |
| --- | --- |
| `h` / `l`, left / right | Navigate requests or notices |
| `j` / `k`, up / down | Select an answer or move through detail pages |
| `1`–`9`, Enter | Select one of the first nine choices; confirm the selection |
| `c` / `o` | Open request context / selected answer detail |
| `d`, Tab, `p` then `0`–`5` | Defer or restore; switch queues; override importance |
| `n`, `i`, `m` | View completion notices; open operator inbox; compose an instruction |
| `q`, Escape | Return from a view or quit; Escape cancels editing |

The answer list follows the selection when there are more choices than fit in the estimated available rows; resize triggers a redraw. Long wrapped text and very small panes can still displace content. Importance defaults to 0; higher values appear first, with submission order for ties. Deferral is durable and does not answer or automatically resurface a request. The queue abbreviates long question, explanation and answer text; `c` pages through the full question and context, and `o` pages through the selected answer and recommendation reason. Terminal control characters are rendered as spaces.

## Messages and notices

Create a channel with its participant labels, then send a direct message:

```sh
./target/debug/aviary call 127.0.0.1:7432 '{"op":"create_channel","channel":{"name":"team","members":["agent-a","agent-b"]}}'
./target/debug/aviary call 127.0.0.1:7432 '{"op":"send_message","message":{"identity":"example-message-1","channel":"team","sender":"agent-a","target":{"kind":"participant","name":"agent-b"},"body":"Please review","context":"Work description","activity":"task-1"}}'
./target/debug/aviary call 127.0.0.1:7432 '{"op":"inbox","participant":"agent-b"}'
```

Channel membership is immutable. A group target, `{"kind":"group"}`, addresses every member except the sender; recipient membership is captured at acceptance. Inbox reads are non-destructive. For large inboxes use `{"op":"inbox_page","participant":"agent-b","offset":0}` and follow `next_offset` until it is null. For history, call `history_page` with the root `kind` (`decision` or `message`), `id`, `section` (`decisions`, `messages`, or `outcomes`) and an initial `offset` of 0; follow `next_offset` separately for each section. A direct message can set `"expects_reply":true`; its recipient uses `reply_message`, and the requester can retrieve the response with `get_message_response` or `aviary wait-message ADDRESS ID`.

Press `i` in the terminal to open the non-destructive `operator` inbox. Use `h`/`l` to move between messages and `j`/`k` to read their pages, including sender, work context, message and activity; `i` or `q` returns. The inbox has no unread marker or correlated reply control, and it loads all pages into the terminal while open. Use the CLI for a correlated reply or history.

Agents can submit a `submit_notice` with an identity, agent, work and result. The interface shows open notices and retains dismissed ones. To send an instruction from the interface, create a channel containing the `operator` label, press `m`, complete the fields and confirm the review. Long fields show their trailing text while editing; use `j`/`k` to inspect every review page before sending. A failed or uncertain send retains the draft while the interface remains open; the draft is not saved across exit.

The [CLI and operation reference](docs/cli.md) documents every current operation, result shape and an executable workflow. The [portable agent skill](skills/aviary/SKILL.md) contains CLI examples for decisions and messaging and can be copied to a compatible agent or supplied as instructions. It does not require MCP.

## Reliability and interface limits

The service uses ZeroMQ ROUTER/DEALER over TCP and SQLite WAL with `synchronous=FULL`. A successful application response follows the durable commit. On a timeout or disconnect, acceptance is unknown: retry the **same content with the same identity**. An identical retry returns the original ID; conflicting content or a different submission type under that identity is rejected. Keep the ID for later retrieval. The intended durability boundary covers service or OS crash with intact storage and a functioning storage stack; storage loss and backup recovery are outside it. Local process-kill tests do not establish OS-crash or power-loss behavior.

The terminal keeps its current view and retries reads when polling fails. A failed decision or other action is shown as not confirmed; check current state before retrying, because a lost reply can hide an accepted action. It does not automatically resubmit decisions. Queue refreshes and submissions run in the background. While an action is pending, the interface ignores further actions and shows that the outcome is unknown; Ctrl-C requests exit, which may wait for the transport timeout. Unsent drafts do not survive interface exit. The CLI prints JSON on success and exits nonzero on errors. Requests and responses are limited to a 1 MiB ZeroMQ frame. The terminal uses bounded pages for decisions and notices, but offset pages can shift as a queue changes, a near-limit individual item can still fail, and terminal aggregation still materializes full queues and the open operator inbox. Inbox, decision, notice and history page responses read records incrementally. Legacy unpaged list, inbox and history operations can exceed the frame limit; inbox and history page operations provide bounded wire responses. The version-1 envelope and CLI schema are provisional; update service, CLI and skill together. The [architecture](docs/architecture.md) and [decisions](docs/decisions.md) describe the contract and remaining choices.

## Verify

```sh
cargo test --locked
python3 tests/terminal_pty.py
python3 tests/terminal_reconnect.py
python3 tests/skill_smoke.py
python3 tests/cli_reference.py
python3 tests/tmux_smoke.py  # requires tmux
```

Build first for the Python 3.10+ smoke tests. They use an isolated temporary database and loopback sockets; the terminal test requires a POSIX pseudo-terminal. The tmux smoke test starts its own isolated tmux server. These checks exercise actual client workflows but do not replace the two-machine, OS-crash, platform and operator checks in the [verification plan](docs/verification.md).

Further project documents: [requirements](docs/requirements.md), [architecture](docs/architecture.md), [decisions](docs/decisions.md), [verification](docs/verification.md), and [implementation backlog](docs/tickets.md).
