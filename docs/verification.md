# Verification and validation plan

**Status:** Initial plan; tests are proposed, not executed. [Requirements](requirements.md) contain requirement-specific acceptance criteria. [Traceability](traceability.md) maps every SYS ID to a planned test and every story to its disposition.

Verification determines whether the specified behavior is met; validation determines whether the workflow meets operator needs. Execute each requirement’s acceptance criterion under documented environment, setup, inputs and expected results; record version, result, evidence and deviations. `T-###` maps one-to-one to `SYS-###`; a test may use multiple cases and a shared fixture. Inspection checks documentation and the observable contract, demonstration exercises the actual workflow, and tests assert behavior. No test is passed merely because it is planned.

Use unit and integration tests for structured data, correlation, deduplication, access checks and state transitions; a real CLI and terminal session for interaction; and at least two separately networked machines for distribution and reconnect. Exercise crash after acknowledged persistence, restart service and requester separately, repeated retries, concurrent answers, unauthorized access, terminal closure and reconnect. Preserve logs and observable state as evidence without exposing credentials. The crash oracle only covers the agreed storage and failure assumptions; power loss, disk loss, partitions and credential compromise require explicit additional decisions and tests.

Measure update delay, setup effort, accessibility, payload limits and retention once their acceptance thresholds are agreed. Until then, these remain verification gaps, not implicit performance promises. Test terminal environments and platform support against a reviewed support matrix. Validate attention reduction, one-confirmation acceptance, context switching and resumed agent work with representative operators; record observations and review findings instead of treating subjective satisfaction as a unit-test result.

## Smallest end-to-end milestone

A narrow executable slice: an agent on one machine submits a decision with a stable submission identity and recommended option; the communication service acknowledges durable state; an operator on another machine sees it in a terminal and confirms the recommendation; the waiting agent receives one correlated structured answer. Restart the service and requester around acknowledged state, retry the submission and recover the same answer without a duplicate. Use an explicitly configured network listener on a trusted test network for the cross-machine part; independently verify that the default listener is localhost-only. Demonstrate the interface may close independently. This slice exercises SYS-001, 007–008, 011, 015–016, 021, 023, 025–026, 030, 036, 043–044 in part; it does not claim the complete V1 requirements or all failure modes. Review the deployment and durability assumptions before implementation; authentication, authorization and multi-user isolation are future tests.

The stakeholder approved the initial requirements and architecture direction on 2026-11-09. First-slice work is tracked in [tickets](tickets.md). Local process-kill and retry integration coverage is implemented; it does not constitute complete requirement acceptance. Two-machine testing and operator validation remain outstanding.

## Local ZeroMQ transport evidence (2026-11-09)

At commit `d72a164` plus the following test increment, `cargo test --locked --test slice` passed on the local Linux host over IPv4 loopback. The test sends two outstanding requests on one DEALER and checks both correlation values; observes a submission in an independent list before discarding its unread reply, then retries the same identity and checks the original ID and a single row; and keeps a DEALER open while killing and restarting the service, then retrieves the persisted record through the reconnected socket. The existing test also exercises process-kill recovery, conflicting retries, eight concurrent submitters and competing answers. These checks do not establish behavior under OS crash, network partitions, saturated queues, a real remote machine, or operator terminal use; an unread reply is not a simulation of every network failure. Queue/backpressure, unavailable-service and shutdown checks remain open.


## Decision terminal smoke test (2026-11-09)

On this Linux host, `cargo test --locked` passed keyboard-state and service tests, and `python3 tests/terminal_pty.py` exercised the real client and service over loopback in a pseudo-terminal. It verified no answer before confirmation, Enter on a preselected recommendation, numeric selection of a different suggestion, custom text including Unicode without a recommendation, structured retrieval, and restoration of terminal settings on normal exit. This is not representative operator validation or a supported-platform and tmux matrix; navigation in an actual interactive terminal, large-content presentation, abnormal termination and accessibility remain to be evaluated.


## Local channel-routing evidence (2026-11-09)

`cargo test --locked` passed on the local Linux host. The channel integration test creates a three-member channel, checks direct and group deliveries from two senders to multiple recipients, excludes an unrelated inbox, checks identity conflict and idempotent retry, and verifies retained routing and context after a service process restart. It exercises routing but does not prove authentication, authorization, cross-machine operation, an OS crash, pagination, operator composition, or agent request/reply.


## Local agent request/reply evidence (2026-11-09)

`cargo test --locked` passed a local ZeroMQ integration test that starts a `wait-message` CLI before a directly addressed response, checks the correlated response and sender inbox, rejects an unrelated responder label and a second response, retries an identical response, then restarts both service and waiter and retrieves the same response. Label checks do not establish authentication. Group request/reply, network partitions, remote machines and OS-crash behavior are not covered by this test.


## Local decision priority and deferral evidence (2026-11-09)

`cargo test --locked` passed priority ordering across agents, submission-order ties, a persistent override, active/deferred separation, and service restart recovery. `python3 tests/terminal_pty.py` additionally deferred a decision, closed and reopened the interface, switched to the deferred view, changed priority, and restored it, with terminal settings restored on normal exit. This does not establish completion-notice behavior, the full attention interface, representative operator validation, tmux, other platforms or OS-crash recovery.


## Local explanation and context evidence (2026-11-09)

`cargo test --locked` passed structured submission, retry and listing assertions for explanation and context. `python3 tests/terminal_pty.py` showed an explanation, opened a multi-page supporting context and reached its final marker without submitting an answer. This is local pseudo-terminal evidence, not terminal-size, accessibility, tmux, representative-operator or full-payload-limit validation.


## Local completion-notice evidence (2026-11-09)

`cargo test --locked` passed notice submission, retry conflict, process-restart recovery, dismissal and retained dismissed-history checks. `python3 tests/terminal_pty.py` submitted a notice while the interface was open, observed its count update, checked displayed work and result, and dismissed it. These are local checks, not representative-operator, cross-machine or OS-crash validation.


## Local operator instruction evidence (2026-11-09)

`cargo test --locked` passed, and `python3 tests/terminal_pty.py` composed and confirmed direct and group instructions, checked the expected recipient inboxes, and verified that a rejected send retains its draft and retry identity for another attempt. This does not establish recovery of an unsaved draft after terminal exit, real network failure, representative-operator validation or cross-machine operation.


## Local request history evidence (2026-11-09)

`cargo test --locked` passed a local ZeroMQ test that creates a decision and answer, a related agent request and response, a completion outcome and an unrelated outcome; it verifies activity association, direct reply linkage, exclusion of unrelated activity and identical history after a service process restart. It also checks direct request/reply history without an activity. This does not establish chronological cross-kind ordering, pagination, remote operation or OS-crash recovery.


## Local transport failure evidence (2026-11-09)

`cargo test --locked --test slice` passed an unavailable-service case: a TCP port held without a ZeroMQ service caused the CLI call to exit nonzero with `operation timed out` after the configured interval. On Linux, a separate test connected a DEALER, stopped the test service process, observed `EAGAIN` while filling the DEALER's one-message local send high-water mark, then closed the zero-linger socket and killed the stopped service in under five seconds. This checks local backpressure and forced termination only. It does not establish graceful service shutdown, service-side saturation fairness, bounded total memory, lossless delivery, network partition behavior, or remote-machine behavior.


## Local portable skill evidence (2026-11-09)

The skill-creator `quick_validate.py` accepted `skills/aviary`. After building, `python3 tests/skill_smoke.py` executed the two shell blocks from that file against a local service, parsed decision and message results, retrieved each, and checked a nonzero missing-request error without MCP. This verifies the documented examples in the local environment; it does not establish behavior for every agent framework, platform, or schema version.


## Local queue paging evidence (2026-11-09)

`cargo test --locked` passed an integration case with four 350,000-character decisions and notices: legacy decision listing exceeded a frame, while paged operations returned all four distinct records across multiple responses. `python3 tests/terminal_pty.py` navigated ten decisions and ten notices whose respective aggregate sizes exceeded a frame. These checks use a stable local queue and do not prove consistent traversal during concurrent mutation, bounded memory, individual near-limit item handling, inbox/history pagination, or remote operation.


## Local inbox and history paging evidence (2026-11-10)

`cargo test --locked --test slice inbox_and_history_pages_cross_frame_limit` passed over loopback. Four large records in each inbox and history section exceeded the unpaged response limit; paging returned four distinct IDs in each section, and an unrelated inbox was empty. This exercises stable data only: pages do not share a snapshot, queries materialize full results, and this does not establish bounded memory, individual near-limit records, remote operation or OS-crash recovery.


## Local recipient reconnect evidence (2026-11-10)

`cargo test --locked --test slice recipient_retrieves_messages_accepted_while_disconnected` passed over loopback. A recipient read an empty inbox, disconnected its DEALER, and reconnected after another client received acceptance for a direct message; the recipient then retrieved the accepted ID and body, while the sender's inbox remained empty. This exercises an explicit socket disconnect on one host, not a network partition, another machine, or OS-crash recovery.


## Local signal shutdown evidence (2026-11-10)

`cargo test --locked --test slice signal_shutdown_preserves_acknowledged_submissions` passed on Linux over loopback. After a submission was acknowledged, the idle service exited successfully within five seconds of SIGTERM and later SIGINT. Each restart recovered the same ID and an identical retry returned it. This does not test a signal during an operation or a saturated peer, establish a general shutdown deadline, or simulate an OS crash or remote connection.


## Local in-flight shutdown evidence (2026-11-10)

`cargo test --locked --test slice signal_shutdown_waits_for_in_flight_write` passed on Linux over loopback. The test held an independent SQLite write lock, observed the service start a submission and receive SIGTERM, verified the process remained alive, then released the lock. After successful service exit and restart, the submission was present before retry; retry returned its original ID with one record. The test does not establish delivery of the original reply, saturated-peer shutdown behavior, a general deadline, an OS crash, or remote behavior.


## Local tmux decision evidence (2026-11-10)

On Linux 7.2.11-200.nobara.fc44.x86_64 with tmux 3.7c and Python 3.14.7, `python3 tests/tmux_smoke.py` started an isolated service and tmux server using a clean configuration and a 100×30 pane. The captured pane showed the recommended answer first and selected; the test checked no answer existed before Enter, confirmed it, retrieved the expected structured answer, and exited the client with q. This is an automated local tmux demonstration, not a supported terminal matrix, a physical terminal or representative operator validation; narrow dimensions and broader navigation remain unverified.


## Local choice viewport evidence (2026-11-10)

`python3 tests/tmux_smoke.py` passed with tmux 3.7c on local Linux. After a recommended decision, it submitted a request with twelve short options, resized the pane from 100×30 to 100×15, observed a redraw showing the first five choices, navigated to the visible thirteenth custom choice, and submitted and retrieved its exact text. `python3 tests/terminal_pty.py` also passed the existing local terminal workflows. This is not proof of usable layout for long wrapped content, smaller dimensions, other terminals, accessibility, or representative operators.


## Local narrow detail evidence (2026-11-10)

`python3 tests/tmux_smoke.py` passed on local Linux with tmux 3.7c. In a 40×12 pane, the test opened a request with long supporting context, observed page navigation instructions and reached the final marker without answering. The existing `python3 tests/terminal_pty.py` suite also passed. This checks one ASCII context and terminal size, not every Unicode width, long header, resize during reading, notice contents, or a complete supported terminal matrix.


## Local long answer evidence (2026-11-10)

`python3 tests/tmux_smoke.py` passed locally with tmux 3.7c. In a 100×15 pane it submitted a long recommended answer, observed the recommendation label in its abbreviated preview, opened the selected answer detail, navigated to its final marker, verified no answer had been submitted, then confirmed it and retrieved the exact full value. This does not establish handling of all Unicode widths, long editor fields, very small panes or representative operator acceptance.


## Local long instruction evidence (2026-11-10)

`python3 tests/tmux_smoke.py` passed on Linux with tmux 3.7c. In a 40×12 pane it composed a long direct instruction, observed the draft tail and review controls, verified the recipient inbox was empty before confirmation, paged to the instruction end, then confirmed and retrieved the exact text. The existing pseudo-terminal instruction cases passed as well. This does not validate every Unicode width, resizing during review, actual network failure, other terminals, or representative operators.
