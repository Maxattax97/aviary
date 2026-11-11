<p align="center">
  <img src="assets/aviary.svg" width="112" alt="Aviary logo">
</p>

# Aviary

Aviary is a communication service and terminal attention interface for a human operator and agents working across processes or trusted machines. Agents explicitly submit decisions when they need operator action; ordinary messages stay in addressed inboxes and completion notices appear separately. Agents can request decisions, exchange targeted messages, report completed work, and recover accepted communications after a service restart. The service runs independently of the terminal interface and does not manage agent execution.

**Status:** Aviary is an evolving prototype, not a V1 release. The core workflows below work locally; two-machine, OS-crash, supported-platform and representative-operator acceptance are still open. The protocol and CLI are provisional. See [verification](docs/verification.md) for actual evidence and [tickets](docs/tickets.md) for remaining work.

**Terminal:** The interface uses terminal-scheme-aware colors with basic-color and monochrome fallbacks; broader terminal and theme validation remains open; see the [terminal verification matrix](docs/terminal-support.md). Press `?` for keyboard help, or Ctrl-G during text entry; Esc closes help without discarding the view or draft.

**Trust boundary:** The service has no authentication, authorization or encryption. Any process that can connect can read communications, impersonate a participant and submit answers. It listens on `127.0.0.1:7432` by default; expose it only to an explicitly trusted network. Participant and channel names are routing labels, not security identities.

## Build and start

A Rust toolchain and C/C++ compiler are required for a source build. The target platforms are Linux, macOS and BSD, including glibc and musl Linux environments; verification currently focuses on Fedora first. Other targets are not yet verified. The terminal is intended to work directly, in tmux and in Herdr panes.

```sh
cargo build --locked
./target/debug/aviary tui
```

With no address, `tui` starts or reuses the service on `127.0.0.1:7432` with `aviary.sqlite` in the current directory. It checks the service protocol and canonical database path before reuse. The service continues after the interface exits and writes `aviary-service.log` in that directory. `aviary tui --local-address 127.0.0.1:PORT` uses another IPv4 loopback port for the same workflow; an explicitly supplied positional address connects only and never starts a service. A conflicting or unidentifiable listener is an error. This identity check is not authentication.

Run `aviary --help` or `aviary <command> --help` for the command-line interface.

To explore the interface with sample work, open `aviary tui` once, then run this in another terminal:

```sh
python3 scripts/seed_demo.py
./target/debug/aviary tui
```

The script adds five decisions, five operator messages and three completion notices to the service at `127.0.0.1:7432`. Pass a different `host:port` to seed another service. The sample records persist in that service's database, and no agents are actually waiting for replies. Each run adds a fresh set; use a disposable database if you do not want sample records in your regular one.

The service defaults to `aviary.sqlite` and `127.0.0.1:7432`. To choose both the database and listener:

```sh
./target/debug/aviary serve /path/to/aviary.sqlite 192.168.1.10:7432
```

Use an address assigned to the host and a trusted network. Keep the service running while clients connect; closing the interface does not stop it. Stop a foreground service with Ctrl-C or SIGTERM; for an automatically started service, obtain its current PID using `aviary call 127.0.0.1:7432 '{"op":"service_info"}'` and send SIGTERM to that process. It stops taking new work after observing the signal and lets a request already being processed finish; replies may still be lost, so retry unknown outcomes with the same identity. A service takes an advisory exclusive lock on its database file and refuses a second Aviary service using the same file, including hard-link and symlink aliases; use a separate database for a separate service. The lock does not prevent non-Aviary programs from changing the database. The SQLite database and its WAL can contain private work: protect their directory and do not remove the WAL while the service is running.

## Ask for a decision

In another terminal, submit a request. Choose a new deployment-wide `identity` for each new submission; the recommendation is a zero-based index.

```sh
./target/debug/aviary call 127.0.0.1:7432 '{"op":"submit","request":{"identity":"example-decision-1","agent":"agent-1","question":"Proceed?","options":["No","Yes"],"recommendation":1,"reason":"Checks passed"}}'
./target/debug/aviary tui 127.0.0.1:7432
```

The first command returns a JSON `id`. Replace `1` below with that ID to wait for an answer or deferral, or retrieve the current answer later:

```sh
./target/debug/aviary wait 127.0.0.1:7432 1
./target/debug/aviary call 127.0.0.1:7432 '{"op":"get","id":1}'
```

The interface presents a recommendation first and preselects it, but Enter is an explicit decision. A shortened `[rec]` label marks a recommendation when its full reason does not fit; `o` opens the answer and reason. Requests can have no suggested answers; the operator can select another suggestion or enter a custom answer. Enter opens the custom answer inline; Enter submits it and Esc cancels editing. Ctrl-O suspends the answer for navigation and resumes it on its original decision; one answer draft can be held at a time. If that decision is no longer on the refreshed page, the terminal retains the draft and warns before any explicit submission. The draft remains in memory only. The response distinguishes suggested and custom answers. A second decision is rejected.

| Key | Action |
| --- | --- |
| Tab | Switch focus between the item list and details |
| `h` / `l`, left / right | Navigate requests, messages or notices |
| `j` / `k`, up / down | Move through the focused list, select an answer, or scroll message and notice details |
| Page Up / Page Down | Scroll the focused details |
| `1`–`9`, Enter | Select one of the first nine choices; confirm the selection |
| `c` / `o` | Open request context / selected answer detail |
| `H` / `g` | Open history for the current request or inbox message / look up history by kind and ID |
| `/` | Search retained messages; Enter opens the selected result |
| `v` | Show the operator overview |
| `d`, `p` then `0`–`5` | Defer to the bottom of this priority; override importance |
| `n`, `i`, `m` | View completion notices; open operator inbox; compose an instruction |
| `q`, Escape | Return from a view or quit; Escape cancels editing |

The focused region is labeled and the selected item is marked with `>`. The list and details appear side by side in wider terminals and stack in narrower panes. The interface asks for at least 40 × 12; other size and terminal support remains under verification. Details scroll by visible lines, and the selected answer follows navigation. On resize, the interface maps the first visible text position into the newly wrapped details; a wider pane can show earlier text, and grapheme clusters are kept together, but terminal-specific widths need broader verification. Importance defaults to 0; higher values appear first and ties begin in submission order. Deferring keeps the decision pending, moves it to the bottom of its priority, and records a notice retrievable by the requesting agent. An agent can swap two decisions at equal importance using the documented queue operation. The list abbreviates long questions; `c` pages through the full question and context, and `o` pages through the selected answer and recommendation reason. Terminal control characters are rendered as spaces.

Click the list or detail pane to focus it, click a list row to select it, or click the inbox and notices labels to navigate. Mouse clicks do not submit actions.

A brief `New decision received` notification can appear even if the decision belongs on another queue page; it does not move the current selection or draft. It is local to the open interface and can be delayed while reads pause or fail.

## Messages and notices

Create a channel with its participant labels, then send a direct message:

```sh
./target/debug/aviary call 127.0.0.1:7432 '{"op":"create_channel","channel":{"name":"team","members":["agent-a","agent-b"]}}'
./target/debug/aviary call 127.0.0.1:7432 '{"op":"send_message","message":{"identity":"example-message-1","channel":"team","sender":"agent-a","target":{"kind":"participant","name":"agent-b"},"body":"Please review","context":"Work description","activity":"task-1"}}'
./target/debug/aviary call 127.0.0.1:7432 '{"op":"inbox","participant":"agent-b"}'
```

Channel membership is immutable. A group target, `{"kind":"group"}`, addresses every member except the sender; recipient membership is captured at acceptance. Inbox reads are non-destructive. For large inboxes use `{"op":"inbox_page","participant":"agent-b","offset":0}` and follow `next_offset` until it is null. For history, call `history_page` with the root `kind` (`decision` or `message`), `id`, `section` (`decisions`, `messages`, or `outcomes`) and an initial `offset` of 0; follow `next_offset` separately for each section. A direct message can set `"expects_reply":true`; its recipient uses `reply_message`, and the requester can retrieve the response with `get_message_response` or `aviary wait-message ADDRESS ID`.

Press `i` in the terminal to open the non-destructive `operator` inbox. Use `h`/`l` to move between messages and `j`/`k` to scroll their details, including sender, work context, message and activity; `i` or `q` returns. For a direct message requesting a response, press `r`, enter a deployment-unique reply identity and text, then review and confirm. After a failed or uncertain send, retry the unchanged draft with the same identity; the draft lasts only while the interface remains open. A request may already have been answered even if the reply control is shown. The inbox has no unread marker and loads one response page at a time; moving across a page boundary may briefly show a loading state. The list indicates when more records are available without claiming a total.

Press `m` to compose an instruction; the destination is shown as group channel `all`. Press Enter to send. Left and right arrows, Home and End move through the single-line draft. Provision `all` explicitly with `operator` and at least one intended recipient using the CLI; Aviary does not change its membership. Group delivery addresses the other members at acceptance and does not establish that any agent is monitoring its inbox. If the send is rejected or unconfirmed, the open draft is locked with its exact content and identity for an explicit Enter retry; Esc asks before discarding it. Ctrl-O suspends the draft while navigating, and `m` on the decision screen resumes it. It does not survive terminal exit. Direct and group messages remain available through the CLI.

Agents can submit a `submit_notice` with an identity, agent, work and result. The interface shows open notices and retains dismissed ones. It loads one notice response page at a time; a `+` after the count means additional notices exist, and h/l loads adjacent pages.

In the terminal, `v` shows a snapshot of pending decisions, retained operator messages and open completion notices. Retained messages include previously read items; they are not an unread count. The overview refreshes on change hints and displays unavailable while disconnected. `/` searches retained message bodies, senders, context and activity using a literal substring (ASCII case-insensitive). Enter opens the selected result; `n`/`p` change result pages, `e` edits the query, and Esc closes search. Search results and history pages are separate snapshots. `H` opens history for the current decision or inbox message. `g` looks up a decision or message by ID, including completed records. In history, `1`/`2`/`3` select decisions, messages or outcomes; arrow keys and Page Up/Down scroll through records, `R` retries a read, and `q` returns. Records are displayed together as JSON and grouped by section, not a global timeline. Additional bounded pages load when scrolling past a boundary; each is a separate snapshot, so concurrent changes can shift offsets.

Run `aviary watch ADDRESS` to stream agent-facing connection and change hints; reconcile against the durable queue or inbox on connection and after a hint. Hints can coalesce and the local revision is not a durable cursor.

The [CLI and operation reference](docs/cli.md) documents every current operation, result shape and an executable workflow. The [portable agent skill](skills/aviary/SKILL.md) contains CLI examples for decisions and messaging and can be copied to a compatible agent or supplied as instructions. It does not require MCP.

For larger operations, use `aviary call ADDRESS - < operation.json` to read the JSON from standard input instead of passing it as a command-line argument. The input and wire-frame limits are described in the CLI reference.

## Reliability and interface limits

The service uses ZeroMQ ROUTER/DEALER over TCP and SQLite WAL with `synchronous=FULL`. A successful application response follows the durable commit. The service does not wait for a disconnected or full reply queue; a reply may be lost even after commit. On a timeout or disconnect, acceptance is unknown: retry the **same content with the same identity**. An identical retry returns the original ID; conflicting content or a different submission type under that identity is rejected. Keep the ID for later retrieval. The intended durability boundary covers service or OS crash with intact storage and a functioning storage stack; storage loss and backup recovery are outside it. Local process-kill tests do not establish OS-crash or power-loss behavior.

The terminal subscribes to change hints and rereads durable state on a hint or reconnect. A disconnected subscription is shown, and a failed read can be retried with `R`. A failed decision or other action is shown as not confirmed; check current state before retrying, because a lost reply can hide an accepted action. It does not automatically resubmit decisions. Queue refreshes and submissions run in the background. While an action is pending, the interface ignores further actions and shows that the outcome is unknown; Ctrl-C or an external SIGINT or SIGTERM requests exit, which may wait for the transport timeout. The terminal restores its settings on these exits; unsent drafts do not survive exit. The CLI prints JSON on success and exits nonzero on errors. Version-2 requests are limited to 1 MiB and responses to 3 MiB per ZeroMQ frame; version-1 responses remain limited to 1 MiB. Upgrade the service before clients. The terminal loads one decision and notice page at a time; h/l crosses page boundaries. A plus sign indicates more decisions beyond the current page. Offset pages can shift as a queue changes; exceptionally large records imported from older databases can still exceed a response frame. Inbox, decision, notice and history page responses read records incrementally. Legacy unpaged list, inbox and history operations can exceed the frame limit; inbox and history page operations provide bounded wire responses. The transport envelopes and CLI schema are provisional; update service, CLI and skill together. The [architecture](docs/architecture.md) and [decisions](docs/decisions.md) describe the contract and remaining choices.

## Verify

```sh
cargo test --locked
python3 tests/terminal_pty.py
python3 tests/terminal_render.py  # requires tmux
python3 tests/terminal_lost_ack.py  # requires system libzmq
python3 tests/terminal_arrivals.py
python3 tests/terminal_signals.py
python3 tests/decision_slice.py
python3 tests/attention_smoke.py
python3 tests/terminal_reconnect.py
python3 tests/terminal_partial_refresh.py
python3 tests/startup_smoke.py
python3 tests/storage_limit.py  # Linux per-process file-limit fault fixture
python3 tests/default_listener.py  # Linux procfs; skips if port 7432 is occupied
python3 tests/skill_smoke.py
python3 tests/cli_reference.py
python3 tests/cli_stdin.py
python3 tests/zeromq_interop.py  # requires system libzmq
python3 tests/two_machine_smoke.py  # local rehearsal only
python3 tests/tmux_smoke.py  # requires tmux
```

Build first for the Python 3.10+ smoke tests. They use an isolated temporary database and loopback sockets; the terminal test requires a POSIX pseudo-terminal. The tmux smoke test starts its own isolated tmux server; the interoperability test requires a system libzmq shared library. These checks exercise actual client workflows but do not replace the two-machine, OS-crash, platform and operator checks in the [verification plan](docs/verification.md).

Further project documents: [requirements](docs/requirements.md), [architecture](docs/architecture.md), [decisions](docs/decisions.md), [verification](docs/verification.md), and [implementation backlog](docs/tickets.md).
