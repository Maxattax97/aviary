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

The `durable_retry_and_single_answer` integration test additionally races eight local clients submitting suggested and custom answers. It checks that exactly one succeeds, each other client receives `request already answered`, and the stored structured answer matches the successful client before and after a service restart. Retrying that winning answer is also rejected and leaves the stored result unchanged. This tests the service's local serialized request path, not multiple service instances, a remote network, or an OS crash.


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


## Local long custom answer evidence (2026-11-10)

`python3 tests/tmux_smoke.py` passed on Linux with tmux 3.7c. In a 40×12 pane the test entered a long custom answer, opened its paged review with Ctrl-R, observed the end of the draft and verified no answer had been submitted, returned to editing, then submitted and retrieved the exact value. This does not establish draft recovery after exit, all Unicode widths, other terminals, or representative-operator acceptance.


## Local terminal reconnect and live arrival (2026-11-10)

`python3 tests/terminal_reconnect.py` passed on Linux over loopback. It killed the service while a custom draft was open, observed the terminal report polling failure, attempted submission while offline and observed an unconfirmed outcome, restarted the service, confirmed that no answer had been stored, and explicitly submitted the retained draft. It then observed a new request arrive in the already open interface without a manual refresh, and separately exited the interface while the service was offline. This does not simulate a lost reply after commit, prove automatic answer retry, bound multi-page or submission input latency, or establish remote-machine or OS-crash behavior.

The same local terminal test now also defers the request from a second client while its custom draft is open, observes the missing-request warning and retained text, restores the request, and submits the original draft. This does not establish draft persistence after leaving the editor or closing the interface.


## Local stalled-poll responsiveness (2026-11-10)

`python3 tests/terminal_reconnect.py` passed on Linux. A local TCP listener accepted a terminal refresh connection but did not perform the ZeroMQ handshake; while that connection remained open, the test sent m and observed the instruction composer within 750 ms. The test also passed the service-interruption and live-arrival cases. The existing pseudo-terminal and tmux smoke suites and `cargo test --locked` passed. This is not a general responsiveness or latency guarantee: submission calls can block the input loop, and stale-result races and remote partitions need broader testing.


## Local pending-action evidence (2026-11-10)

`python3 tests/terminal_reconnect.py` passed on Linux over loopback. With the service stopped, it submitted a custom draft, observed the pending/unknown indicator, sent Enter again while the operation was pending, and observed the unconfirmed result. After restart no answer was present until an explicit new confirmation. The pseudo-terminal and tmux smoke suites also passed. This does not simulate an action committed with its reply lost, prove cancellation on exit, or cover all event-order races and remote failures.


## Local message and reply lost-ack recovery (2026-11-10)

`cargo test --locked --test slice message_and_reply_lost_ack_retry_after_restart` passed on Linux over loopback. For a direct request and its reply, the test independently observed the committed record, discarded the unread application response, killed and restarted the service, and retried the same identity and content. Each retry returned the original ID; changed-content retries conflicted, a different second reply was rejected, and each recipient inbox retained one unchanged record. This demonstrates two local process kills and unread replies, not arbitrary network loss, a service acknowledgment observed before a machine crash, OS-crash durability, or remote-machine behavior.


## Local incremental page evidence (2026-11-10)

`cargo test --locked --test slice streamed_pages_match_unpaged_order_and_offsets` compared decision, deferred, notice and inbox pages with their unpaged results at every offset, including priority overrides, ties, and offsets past the end. The existing frame-limit integration cases returned all records across multiple pages, and the terminal, tmux and skill smoke tests passed locally. These checks confirm observable ordering and paging for the fixtures, not a measured process-memory bound, SQLite sort or scan costs, concurrent offset consistency, history pagination, or remote behavior.


## Local incremental history evidence (2026-11-10)

`cargo test --locked --test slice request_history_links_decisions_messages_and_outcomes` passed with page results compared to unpaged history across all three sections and each offset for decision, request and reply roots, including direct links without activity, missing roots and offsets past the end. The existing frame-limit case checks multi-page retrieval. These fixtures do not measure memory or SQLite scan costs, verify consistent traversal under concurrent mutation, or establish remote and OS-crash behavior.


## Local CLI reference workflow (2026-11-10)

After `cargo build --locked`, `python3 tests/cli_reference.py` extracted and ran the Python example from `docs/cli.md` against an isolated loopback service. It exercised channel creation, decision submit and retry, answer and retrieval, direct message, reply and retry, completion notice and linked history; `python3 tests/skill_smoke.py` also passed. The operation table was checked against the current implementation but is not exhaustively generated or tested for every parameter, failure, page boundary, platform or agent framework.


## Local operator inbox evidence (2026-11-10)

`python3 tests/terminal_pty.py` passed on Linux over loopback. It closed the terminal, sent an operator-to-agent request and a response, reopened the interface, and displayed the response with sender, inherited work context and activity. While that inbox was open it delivered a second message and observed its arrival and context without a manual refresh. Reads remained non-destructive. The test also navigated an operator inbox whose aggregate content exceeded a frame. This is local pseudo-terminal evidence, not a full history view, unread tracking, correlated reply control, bounded terminal memory, remote operation or representative operator validation.


## Local operator reply evidence (2026-11-10)

`python3 tests/terminal_pty.py` passed on Linux over loopback. An agent submitted a direct request to `operator`; the test opened its inbox item, composed a reply, verified no response existed before confirmation, and retrieved the correlated body and inherited activity afterward. An identical retry returned the original reply ID; a different second reply was rejected while its draft remained visible. This does not simulate an actually lost reply, verify remote operation, establish operator identity, or replace representative-operator validation.


## Local clean-directory startup and interface closure (2026-11-10)

`python3 tests/startup_smoke.py` passed on local Linux. Using the already-built executable, an empty temporary working directory and a minimal HOME/PATH/TERM environment, the test started one service with an explicit loopback port and relative database path. It opened and closed the terminal, exchanged a direct request and reply while the terminal was closed, reopened it to view the reply and context, and retrieved both linked records through history. The service remained running until the test stopped it. This is not an installation on a clean OS, a different supported platform, a remote machine, or representative operator validation.


## Local malformed multipart evidence (2026-11-10)

`cargo test --locked` passed a local case sending a valid-looking submission followed by 96 unexpected 65,536-byte frames on one DEALER message. The service discarded it, accepted the next valid request on that connection, and did not store the malformed submission. A second local test confirmed that the CLI rejected a multipart response from a test ROUTER. The terminal, CLI-reference and clean-directory smoke tests also passed. This does not measure peak memory, bound libzmq buffering or drain time, establish fairness under saturation, or test remote behavior.


## Local slow-peer backpressure evidence (2026-11-10)

`cargo test --locked --test slice slow_peer_backpressure_does_not_block_other_clients` and the full Rust suite passed on Linux loopback. A DEALER with a small receive queue stopped reading replies while issuing 100 requests whose responses included a 700,000-character record. The service logged its per-peer reply queue as full; a second client then submitted and retrieved a separate request. A further submission from the slow peer was independently observed in SQLite with its application reply unread; an identical retry returned the original ID before and after a service process restart. This is a bounded local fixture, not a latency guarantee, proof of lossless reply delivery, aggregate memory bound, arbitrary-load fairness, remote-network or OS-crash validation.


## Local terminal history evidence (2026-11-10)

`python3 tests/history_terminal.py` passed on Linux over loopback. The pseudo-terminal opened a pending decision and an inbox message as history roots, switched among related decision, message and outcome sections, traversed message response pages totaling more than one frame, and verified that merely inspecting did not answer the decision. It then answered the decision and looked up its completed history by ID. This does not establish snapshot consistency across pages, global chronology, representative-operator usability, remote operation or OS-crash recovery.


## Local mixed-agent attention evidence (2026-11-10)

`python3 tests/attention_smoke.py` passed on Linux over loopback. It submitted decisions from two agents at different priorities, an ordinary message addressed to the operator, and a completion notice. The CLI queue and pseudo-terminal showed the two decisions in priority order without the routine message; the message was available in the separate inbox, and the notice appeared in the separate notice view. Neither decision was answered by inspection. This checks explicit operation-based classification for one fixture, not whether real agents choose useful requests or whether the workflow reduces operator attention.


## Local Python/libzmq interoperability (2026-11-10)

`python3 tests/zeromq_interop.py` passed on Linux over loopback using Python ctypes and system libzmq 4.3.5 against the Rust service. One DEALER pipelined a decision submission and list request, matched both correlations, checked an unsupported version error, survived a service process restart, retried the same identity, rejected conflicting content, and retrieved an answer submitted through the CLI. This exercises a different client stack using libzmq, not an independent ZeroMQ implementation, another machine, OS-crash recovery, or a stable future wire contract.


## Local terminal inbox page navigation (2026-11-10)

`python3 tests/terminal_pty.py` passed on Linux over loopback. With thirteen operator inbox messages totaling more than one frame, it navigated forward across a response-page boundary and backward to the previous page, after exercising live arrival, history access, reply and retry on smaller data. `python3 tests/history_terminal.py` also passed with an inbox larger than one frame. These checks establish behavior for stable fixtures, not a measured total memory bound, arbitrary concurrent mutation, remote operation or representative-operator usability.


## Local terminal notice page navigation (2026-11-10)

`python3 tests/terminal_pty.py` passed on Linux over loopback with ten large notices spanning response pages. It navigated forward and backward across a page boundary, dismissed earlier notices through the CLI until the active offset was past the end, observed the terminal fall back to the remaining notices, and dismissed one from the recovered view. The selected decision remained in place. This checks a bounded fixture and one page-shift scenario, not all concurrent mutations, a measured total memory bound, remote operation or representative-operator usability.
