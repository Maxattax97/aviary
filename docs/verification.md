# Verification and validation plan

**Status:** Initial plan; tests are proposed, not executed. [Requirements](requirements.md) contain requirement-specific acceptance criteria. [Traceability](traceability.md) maps every SYS ID to a planned test and every story to its disposition.

Verification determines whether the specified behavior is met; validation determines whether the workflow meets operator needs. Execute each requirement’s acceptance criterion under documented environment, setup, inputs and expected results; record version, result, evidence and deviations. `T-###` maps one-to-one to `SYS-###`; a test may use multiple cases and a shared fixture. Inspection checks documentation and the observable contract, demonstration exercises the actual workflow, and tests assert behavior. No test is passed merely because it is planned.

Use unit and integration tests for structured data, correlation, deduplication, access checks and state transitions; a real CLI and terminal session for interaction; and at least two separately networked machines for distribution and reconnect. Exercise crash after acknowledged persistence, restart service and requester separately, repeated retries, concurrent answers, unauthorized access, terminal closure and reconnect. Preserve logs and observable state as evidence without exposing credentials. The crash oracle only covers the agreed storage and failure assumptions; power loss, disk loss, partitions and credential compromise require explicit additional decisions and tests.

Measure update delay, setup effort, accessibility, payload limits and retention once their acceptance thresholds are agreed. Until then, these remain verification gaps, not implicit performance promises. Test terminal environments and platform support against a reviewed support matrix. Validate attention reduction, one-confirmation acceptance, context switching and resumed agent work with representative operators; record observations and review findings instead of treating subjective satisfaction as a unit-test result. The proposed [operator session](operator-validation.md) provides a concrete, unexecuted protocol.

## Planned default instruction verification (T-045)

Not run: with an explicitly provisioned `all` channel containing `operator` and at least one agent label, open the composer, enter only instruction text and confirm; verify the exact message appears in each other member’s inbox with a generated nonempty identity. Send a second new instruction and verify a different identity. Exercise an unknown send outcome, retry the unchanged draft, and verify one accepted message with the same identity and contents; reject a conflicting reuse. Exercise an absent `all` channel and a channel without an eligible recipient or operator membership: show an unconfirmed/error outcome and retain the draft rather than claim delivery. Verify no message is accepted before confirmation. Existing manual-composer evidence below does not establish T-045. Provisioning, collision strategy, and recovery after closing the interface require implementation decisions and explicit tests.

## Planned on-demand help and automatic startup (T-046, T-047)

**T-046, not run:** inspect normal decision, inbox and composition views for absence of a persistent complete shortcut list; allow concise contextual hints. Open and dismiss complete keyboard help from each view using the chosen binding, verify relevant actions are discoverable, and verify the current selection and any draft survive. Exercise supported pane dimensions and cancellation without sending an instruction or answer.

**T-047, not run:** from an isolated local environment with no service, start the normal operator entry point without separately launching a daemon; submit and retrieve work, exit the interface, communicate with the continuing service, reopen and retrieve the same state. Verify an existing intended service is reused, concurrent launches do not create conflicting instances, data survives restart, and startup or endpoint conflicts produce a clear error without replacing an unrelated service or database. Verify an explicitly remote endpoint is not silently replaced by a local instance. Define and test lifecycle, collision and clean-environment behavior before claiming acceptance. Existing startup evidence used a separately started service and does not satisfy T-047.

## Planned terminal color verification (T-048, T-049)

**T-048, not run:** select and document a standard color system and demonstrate the default interface against at least two different terminal color schemes; check that semantic colors follow the user's selected scheme rather than fixed RGB values. Record terminal, scheme and observation method. Basic ANSI palette colors are a design candidate, not a verified implementation.

**T-049, not run:** exercise the same navigation, selected decision, status/error, on-demand help and decision confirmation in supported basic-color and monochrome configurations that do not advertise 256 colors. Verify the workflows remain readable and actionable without color-only cues and do not depend on 256-color sequences. Test directly and within supported multiplexers where available; record the terminal capabilities and limits rather than claiming that a capability setting proves every physical terminal works.

## Planned operator interface verification (T-050–T-057)

These tests are planned and have not passed. Existing local terminal checks are partial evidence only.

- **T-050:** Populate decisions, messages and completions; verify each overview category, including empty, unavailable and paginated cases, without implying unverified exact counts.
- **T-051:** Create matching and nonmatching retained records; find and open the correct item by content, originating agent and work context. Exercise result paging and no-match behavior under documented matching semantics.
- **T-052:** Browse distinct items and verify the selected-item preview follows selection, offers full details and remains usable at supported pane sizes. Inspection must not change records.
- **T-053:** Traverse decision, inbox and notice views; identify active view, selection and applicable actions via concise hints or on-demand help.
- **T-054:** Exercise an in-flight action, confirmed acceptance, explicit service rejection and a missing response after possible commit. Verify each is distinguished, no unknown result is called accepted or rejected, and no action is automatically resubmitted.
- **T-055:** Navigate long wrapped content by visible lines, resize, and verify the previously read position remains accessible; include Unicode and a supported narrow pane.
- **T-056:** Preserve an exact draft and original item association while opening and closing help/details and during a polling failure and recovery. Verify selection and reading position on return; separately document behavior when the item disappears or the interface exits.
- **T-057:** Inspect target and effect before confirming a decision, instruction and applicable queue action; verify navigation and inspection alone do not submit an action. Exercise concurrent item changes.

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

On local Linux x86-64 with tmux 3.7c and Python 3.14.7, `python3 tests/tmux_smoke.py` started an isolated service and tmux server using a clean configuration and a 100×30 pane. The captured pane showed the recommended answer first and selected; the test checked no answer existed before Enter, confirmed it, retrieved the expected structured answer, and exited the client with q. This is an automated local tmux demonstration, not a supported terminal matrix, a physical terminal or representative operator validation; narrow dimensions and broader navigation remain unverified.


## Local choice viewport evidence (2026-11-10)

`python3 tests/tmux_smoke.py` passed with tmux 3.7c on local Linux. After a recommended decision, it submitted a request with twelve short options, resized the pane from 100×30 to 100×15, observed a redraw showing the first five choices, navigated to the visible thirteenth custom choice, and submitted and retrieved its exact text. `python3 tests/terminal_pty.py` also passed the existing local terminal workflows. This is not proof of usable layout for long wrapped content, smaller dimensions, other terminals, accessibility, or representative operators.


## Local narrow detail evidence (2026-11-10)

`python3 tests/tmux_smoke.py` passed on local Linux with tmux 3.7c. In a 40×12 pane, the test opened a request with long supporting context, observed page navigation instructions and reached the final marker without answering. The existing `python3 tests/terminal_pty.py` suite also passed. This checks one ASCII context and terminal size, not every Unicode width, long header, resize during reading, notice contents, or a complete supported terminal matrix.

The tmux test also checks a short recommendation at 40×12: its queue header, question, selected `[rec] Yes` option and confirmation instruction are visible together, no answer is stored before Enter, and Enter stores the recommended answer. This single fixture does not establish general layout or accessibility support.


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

`python3 tests/terminal_signals.py` passed on local Linux using a pseudo-terminal. With an unsent custom draft open, external SIGINT and SIGTERM each caused normal client exit and restored the saved terminal settings; the service retained an unanswered decision. This does not establish restoration after SIGKILL, a crash or terminal loss, nor cancellation of an already submitted operation.


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

In a local 40×12 tmux pane, `python3 tests/tmux_smoke.py` displayed a notice with a 209-character agent label. The heading and paging/dismissal controls remained visible, and paging reached the end of the label and result before dismissal. This fixture does not establish layout for every content or terminal size.

The tmux test also opened a long decision context at 100×15, navigated to page 4, and resized to 40×12. The new page 6 retained a marker near the start of the previously displayed text; no answer was submitted by inspection. A unit test checks the same page-start mapping for the other paged views. This does not establish a general terminal-size or Unicode layout guarantee.

## Local terminal decision page navigation (2026-11-10)

`python3 tests/terminal_pty.py` passed on Linux loopback with ten large decisions across multiple response pages. It navigated forward and backward across a boundary, then answered the first seven through another client and observed recovery from an offset beyond the end. The terminal recovery, history and tmux smoke tests and Rust suite also passed. This is one local shift scenario, not consistent traversal during arbitrary concurrent changes, a total memory measurement, remote validation or representative operator acceptance.

## Near-limit response retrieval (2026-11-10)

The local `version_two_retrieves_near_limit_decision_and_answer` integration test submitted a decision whose request frame was just below 1 MiB, retrieved it in a version-2 queue page, answered with the near-limit suggested option, and retrieved the combined request and answer in a version-2 history page through both a direct client and the CLI. The same oversized history page returned a bounded error to a version-1 client. This does not prove every accepted payload shape or historical database import fits, bound total memory, or validate remote operation.

`near_limit_reply_and_inherited_context_survive_restart` also sent a near-limit message context and a separately near-limit reply body. It retrieved the combined reply through version-2 response, inbox and history operations, observed a version-1 size error, then restarted the service and verified identical retry and retrieval. A page-builder unit test checks that an oversized lookahead does not hide preceding valid items; fetching that item itself still fails. These are local fixtures and a service-process restart, not OS-crash or remote evidence.

## Prepared trusted two-machine exercise

`tests/two_machine.py` provides a coordinated decision and message exchange using an explicitly supplied service address and a fresh token. On the service machine, start an isolated test service bound to its selected trusted-network address, then run the origin role against that address. Use a fresh token each time, generated for example with `python3 -c 'import uuid; print(uuid.uuid4().hex)'`. Supply the matching Aviary executable explicitly on each machine:

```sh
python3 tests/two_machine.py origin SERVICE_HOST:PORT TOKEN --binary /path/to/aviary --expect-restart
# After the origin prints "ready", stop the disposable service, wait for
# "unavailable", then restart it with the same database and listener.
# After the origin prints "recovered", on the second trusted machine:
python3 tests/two_machine.py peer SERVICE_HOST:PORT TOKEN --binary /path/to/aviary
```

Start the peer only after `recovered` to test retrieval of a message accepted before that client started. The origin observes an unavailable service and retries the identical decision and message after recovery, requiring their original IDs. Record service process identities and the actual stop/restart separately: an observed timeout alone does not prove a restart. The peer checks the message and decision, submits a suggested answer and a correlated reply, and retries the identical reply. The origin checks the persisted answer and reply. Both roles print IDs and hostnames; retain both outputs with the actual network setup and independently verify that the roles ran on separate machines. The test creates durable records and an immutable channel in the selected database; it does not delete them. Use a dedicated test database and a new token, and limit the service to a trusted network. Hostnames and a successful run alone do not prove identity, isolation, or encryption.

`python3 tests/two_machine_smoke.py` passed with both roles and the service on the same Linux host over loopback, including a service-process kill, observed unavailability, restart with the same database, and identical retries. That validates the exercise orchestration only. The actual two-machine run, default-loopback remote reachability check, explicit non-loopback binding, network interruption, OS-crash recovery and representative operator validation remain open.

## Fresh source build and isolated startup (2026-11-10)

On local Linux x86-64 with Rust/Cargo 1.90.0, a temporary source copy was built into a fresh target directory using `cargo build --locked --offline`. The build used the host's existing Cargo cache and installed toolchain. `python3 tests/startup_smoke.py` then passed using that newly built binary: it started the service in a separate empty directory with a minimal environment, communicated while the terminal was closed, and retrieved the reply and history after reopening it. The copied source excluded `.git`, `target` and `GOAL.md`; ignored local SQLite files were initially copied into the private temporary directory and removed before the startup test, leaving the originals untouched. The temporary source and build were removed after the test.

This verifies a fresh build on one configured host, not installation on a clean OS, missing-dependency bootstrap, another supported platform, remote operation or OS-crash recovery.

## Default listener inspection (2026-11-10)

On local Linux x86-64, `python3 tests/default_listener.py` started `aviary serve` with no arguments in an isolated temporary directory. It matched the process's socket inodes against procfs TCP listeners and found only `127.0.0.1:7432`, then completed a local CLI request. A separate occupied-port check verified that the test skips without connecting to an existing listener. The initial free-port check and service bind are not atomic: if another process races for the port, the test fails rather than claiming success. A skip is not evidence. This is local socket inspection, not an independent remote connection attempt, an explicit non-loopback listener check, or a supported-platform matrix.

## SQLite connection settings (2026-11-10)

`cargo test --locked --bin aviary pooled_connections_use_wal_and_full_synchronous` opened a pre-existing rollback-journal database, held all five pool connections simultaneously, and checked that each reported WAL mode and SQLite `synchronous=FULL` while retaining the existing record. It repeated those checks after closing and reopening the pool. This verifies reported settings and one local migration fixture; it does not prove filesystem or storage-device flush behavior, OS-crash recovery, power-loss durability, or compatibility with every existing database.

## Transaction failure and retry (2026-11-10)

`cargo test --locked --test slice failed_message_and_reply_writes_leave_no_partial_delivery` injected SQLite trigger errors after a message or reply transaction had inserted its earlier rows. The service returned errors, and the test found no retry identity, message or recipient delivery from either failed operation. After removing the faults, the same identities and content succeeded; the reply was retried after a service restart and remained a single correlated response. This checks two deliberately injected SQL errors on local storage, not disk exhaustion, failure at commit, lost error responses or OS-crash durability.

On local Linux x86-64, `python3 tests/storage_limit.py` limited only a disposable service process to 256 KiB per file. A decision was acknowledged before a large notice write returned a SQLite disk I/O error; the service remained alive. After a graceful restart without that limit, the decision retained its original ID, the failed notice was absent, and retrying the same notice identity and content succeeded exactly once. This is one induced file-size-limit failure, not an actual full filesystem, a commit-boundary fault, OS crash, storage-device failure or power loss.

## Independent terminal refreshes (2026-11-10)

`python3 tests/terminal_partial_refresh.py` inserted deliberately oversized legacy records into an isolated local test database. An unreadable notice page produced a warning while an ordinary decision remained visible; after that fixture was removed, an unreadable decision page produced a warning while the operator inbox remained usable. The normal terminal, reconnect, history, tmux and Rust regressions also passed. This checks two local page failures, not arbitrary network or database failures, cross-view snapshot consistency or representative operator use.

`python3 tests/terminal_arrivals.py` passed locally with large decisions spanning multiple response pages. While an operator draft was open on a later page, a newly submitted higher-priority decision set the `NEW` indicator without losing or retargeting the draft. It also detected an active arrival while the deferred view was open, and checked that callers omitting `include_newest` receive the earlier response shape. A unit test checks clearing and re-triggering the indicator. This does not prove a latency bound, remote behavior, or visibility while polling is paused.

The local tmux test additionally checks a failed notice refresh at 40×12: the compact warning, decision heading, question, selected recommendation and confirmation help are visible together, and Enter records the recommendation. It does not cover every combination of failures, text width or pane dimensions.

## CLI standard-input submission (2026-11-10)

`python3 tests/cli_stdin.py` passed on local Linux over loopback. It supplied a JSON operation larger than this host’s single-argument limit but below the request-frame limit through stdin, checked the complete stored context, and verified that an identical retry returned the original ID. Malformed JSON and over-limit stdin exited nonzero without adding a record. The executable CLI reference, portable skill examples, and Rust suite also passed. This does not establish cross-platform command limits, remote operation or OS-crash durability.

## Local decision and recovery slice (2026-11-10)

`python3 tests/decision_slice.py` submitted a recommended decision over local ZeroMQ, started and killed a waiting CLI before an answer, restarted that waiter, and confirmed the recommendation in a pseudo-terminal. The waiter received the structured answer. After a service-process kill and restart, the original submission identity returned the same ID, conflicting content was rejected, a new waiter recovered the answer, and the database contained one request with that identity. This is automated local evidence; it does not demonstrate a real remote machine, OS crash or representative human operator.

## Clap and Ratatui migration (2026-11-10)

`cargo test --locked` passed the Rust suite, including argument parsing for the existing positional commands, stdin marker and invalid arguments. The CLI reference, stdin, and portable skill smoke tests passed against the migrated executable. On local Linux, the terminal, arrivals, independent-refresh, reconnect, signal-restoration, attention, history, startup and decision-slice tests passed using loopback; the tmux smoke test passed at its exercised sizes. The pseudo-terminal helper now interprets screen updates at 80×24 rather than expecting complete lines in the output stream. The Ratatui and direct Crossterm dependencies resolve to one Crossterm version. These checks cover the existing views and selected workflows, not a finished interface redesign, Herdr, other platforms, a clean installation, comprehensive Unicode rendering or representative operator validation.

## Terminal screen initialization (2026-11-10)

`python3 tests/terminal_render.py` reproduced stale characters on a prepopulated tmux screen before the fix. After the fix, it verified the exact empty-queue rows, an arrival and return to an empty queue, blank remaining rows, and restoration of the previous screen on exit. The Rust suite and terminal, arrival, signal and tmux smoke tests passed locally. This verifies the exercised tmux session, not every terminal emulator, Herdr, abrupt termination or all screen sizes.

## Keyboard help increment (2026-11-10)

`cargo test --locked` passed, including a state test for preserving a composer draft and selection across help. Local PTY and tmux tests exercised help in decision, inbox, notice and composer views, including Ctrl-G during text entry and the three help pages at 40×12. Other terminal emulators and smaller sizes remain unverified.

## Quick instruction increment (2026-11-10)

The Rust suite passed with a composer state test for review, distinct generated identities and unchanged retry content. `python3 tests/terminal_pty.py` passed locally over loopback: it exercised the actual help screen during a draft, confirmed no delivery before confirmation, retried after a missing-channel rejection, and observed two distinct messages in both group members' inboxes. Storage integration tests cover durable identical retries and lost acknowledgments; the separate terminal proxy test below exercises a lost composer acknowledgment. `/dev/urandom` availability outside this Linux host remains unverified.

## Terminal action clarity (2026-11-10)

`cargo test --locked`, `terminal_pty.py`, `terminal_render.py`, `terminal_reconnect.py`, `terminal_lost_ack.py`, `tmux_smoke.py`, `cli_reference.py` and `terminal_signals.py` passed locally with loopback access. The screen tests inspect view and selected record IDs, help and preserved drafts; they exercise confirmation, rejection for missing and unsuitable channels without membership changes, and a competing answer. A local ZeroMQ proxy forwarded a message to the service and dropped its committed acknowledgment: the terminal showed it as unconfirmed, ignored another confirmation while pending, and only an explicit identical retry produced acceptance, with one recipient record. A sandboxed Rust run without the required socket access aborted in libzmq; the rerun with loopback access passed. These checks do not establish OS-crash durability, two-machine operation, other platforms, Herdr, all terminal dimensions or representative operator validation. This focused increment is not full V1 acceptance.

## Proposed workflow verification (T-058–T-068; not run)

These are prospective checks for the candidates in [requirements](requirements.md), not results or approval. They do not supersede prior evidence.

- **T-058:** Populate multiple priorities and equal-priority decisions, traverse bounded list responses and compare the reconstructed order with the service's stable-fixture queue. Test empty pages, limits and client recovery from interrupted retrieval; specify concurrent-snapshot limits.
- **T-059:** Swap distinct same-priority positions, including nonadjacent items. Confirm exact before/after order, unchanged other items and effective priorities, persistence across restart, and no mutation of submission payload or retry identity.
- **T-060:** After listing, change priority, answer or reorder one item, then attempt a stale swap. Verify rejection and no partial change; race swaps and examine the final valid ordering. Cover duplicate IDs, cross-priority swaps and missing items.
- **T-061:** With no agent connected, submit to `all`, verify durable acceptance and reconnect retrieval after restart. Simulate a lost acknowledgment and retry unchanged; verify a single active item and conflicting reuse rejection. Cover unavailable service without claiming acceptance.
- **T-062:** Resolve one item, then list active work after refresh and restart; only the resolved item disappears. Verify idempotent retry and any separately agreed retention and audit behavior without deleting unrelated existing records.
- **T-063:** Once claiming and recovery are decided, exercise simultaneous agents, take-up, agent exit, reconnect, failure, resolution and retries against the reviewed distribution policy; inspect no lost work or unintended duplicate claims under the specified guarantees.
- **T-064:** Render real decision, inbox and notice screens with multiple distinct items. Identify focused area, selected item, labeled vertically arranged details and available actions; exercise keyboard focus and a reviewed narrow-pane fallback. Validate the visual hierarchy with an operator.
- **T-065:** Navigate more records than fit in one backend response and content longer than one screen without a page-navigation command. Exercise fetch boundary, slow/error recovery, resize, Unicode and concurrent insert/removal, with honest loading/incomplete states and preserved selection where specified.
- **T-066:** Observe a human-readable accepted notification appear and disappear; verify it does not erase a draft or selection. Separately inject explicit rejection and lost acknowledgment after commit and verify actionable, distinguishable states and no automatic resubmission.
- **T-067:** In the real composer, verify destination and send effect are visible; typing, cursor movement and inspection do not send; the explicit send action does not open a separate review page. Exercise accepted, rejected and unknown outcomes, preserving the exact identity and payload for a user-initiated retry.
- **T-068:** See the amended deferral-event oracle below.

Additionally rerun T-048/T-049 for selected color schemes, basic color and monochrome, and existing channel, durable retry and decision tests to protect current users. Use real rendered screens and representative operator validation; local simulated process failures do not establish OS-crash or two-machine behavior.

## P-04/P-05 pre-implementation baseline (2026-11-10)

Before operator review of the proposed layout, `cargo test --locked` passed
(12 unit tests and 23 integration tests). This is evidence for the existing
implementation only; it does not verify the proposed layout or P-04/P-05
acceptance. The operator subsequently reviewed the mockup and agreed to proceed; the
baseline does not verify the proposed interface.

## P-05 direct instruction increment (2026-11-10)

The updated terminal PTY and isolated tmux regressions passed. A local proxy
lost a committed instruction acknowledgment; the terminal kept the outcome
unconfirmed, did not resubmit automatically, and an explicit identical retry
returned one stored message. That exercise also observed the success notice
disappear. Thirteen unit tests passed, including locked-draft and discard
confirmation coverage. `cargo test --locked -- --test-threads=1` passed all 12
then-current unit tests and 23 integration tests with loopback access; an
initial parallel loopback run had one resource timeout, and sandboxed runs
aborted inside libzmq. These local checks do not establish the remaining
reviewed layout, complete draft navigation, all terminal sizes or operator
acceptance.

The local lost-acknowledgment proxy exercise was extended to suspend the
unconfirmed draft, return to the decision view, and resume it before an explicit
identical retry; it passed with one stored message. Fourteen unit tests passed,
including suspension and identity preservation. This does not establish draft
recovery after exit or the remaining P-04 layout.

## P-04 queue-layout increment (2026-11-10)

Local `cargo test --locked -- --test-threads=1` passed 15 unit and 23
integration tests with loopback access. The terminal PTY regression traversed
multi-page decisions, notices and inbox messages, and the tmux smoke exercised
40 × 12 and wider panes, long notice scrolling and selection. A separate PTY
check with NO_COLOR verified focus and that list navigation and Enter in list
focus do not submit a decision; explicit confirmation in detail focus did.
Arrival, signal restoration, partial-refresh, reconnect, and lost-acknowledgment
regressions also passed locally. These checks do not establish the full color or
platform matrix, exact reading-position retention on resize, all Unicode
rendering, concurrent offset traversal, remote operation or operator acceptance.


## Queue resize and color observation (2026-11-10)

The tmux smoke passed after scrolling a long notice and resizing 40 × 12 to
100 × 15 and back while keeping its marker visible. The serial locked Rust
suite passed 15 unit and 23 integration tests; the CLI reference workflow and
NO_COLOR focus/confirmation PTY test also passed. A local PTY capture showed
Crossterm emitting `38;5;6` for the cyan accent and no such color sequence
with NO_COLOR. This does not prove operation on a basic-color-only terminal;
additional resize cases, Unicode clusters, schemes and terminal environments
remain unverified.

## Basic-color output and suspended-draft quit (2026-11-10)

A PTY capture with TERM=xterm observed the basic cyan SGR and no indexed-color
SGR; with NO_COLOR it observed no cyan SGR and retained focus and selection
cues. A unit test covers a split color sequence and preservation of an
unrelated indexed sequence. The isolated tmux smoke and terminal signal
restoration tests passed. The lost-acknowledgment PTY exercise also verified
that ordinary quit prompts before discarding a suspended unconfirmed draft,
then retained and retried the identical request with one stored message. The
serial locked Rust suite passed 16 unit and 23 integration tests. These tests do
not establish contrast across terminal themes or behavior on every terminal.


## Unicode wrapping and narrow uncertain-draft confirmation (2026-11-10)

A unit test checked wrap boundaries and scalar anchors for a combining mark, a
joined emoji and a wide CJK character. A 40 × 12 tmux exercise rendered a
combining sequence and CJK character in a notice. A local 40 × 12 lost-response
proxy test confirmed that the full submission identity and discard/keep choice
are visible, that no automatic resend occurs, and that an explicit identical
retry retains one stored message. The locked serial Rust suite passed 17 unit
and 23 integration tests; CLI reference and portable-skill examples passed.
The grapheme unit test is not proof that every terminal renders joined emoji
with the same width; representative operator and broader scheme validation
remain open.

The [rendered-screen review captures](mockups/p04-p05-rendered-review.md) were
made in an isolated tmux server with synthetic records at 100 × 24 and 40 × 12.
They are awaiting operator review. Inspection exposed a reply hint on a message
that did not request a reply; the hint now appears only when the selected
message is eligible and no suspended instruction blocks reply composition. The
local terminal PTY regression passed with checks for both eligible and
ineligible messages. Captures do not establish interactive behavior or theme
contrast on their own.

## Two xterm palettes (2026-11-10)

In isolated Xvfb xterm sessions, the same synthetic decision was rendered at
100 × 24 with TERM=xterm and two explicitly configured palettes: dark
background `#101018`, foreground `#eeeeee`, ANSI cyan `#30d6d0`; and light
background `#f7f5ec`, foreground `#202020`, ANSI cyan `#005c78`. Normal and
bold cyan were set to the listed value. Captures were visually inspected and
`PYTHONWARNINGS=ignore python3 tests/theme_render.py` confirmed each configured
accent appeared in its corresponding screen. [Both captures](mockups/p04-p05-rendered-review.md) show focus labels
and selection markers as non-color cues. This is evidence for these two xterm
configurations, not a contrast guarantee for arbitrary themes or terminals;
interactive basic-color and monochrome behavior has separate PTY coverage.

## Outcome feedback with a suspended draft (2026-11-10)

A 40 × 12 lost-acknowledgment proxy exercise suspended an uncertain instruction,
dismissed an unrelated notice, observed `Notice dismissed`, waited for it to
expire, and resumed and retried the exact original submission. The service
retained one message. A separate monochrome PTY check selected a decision,
dismissed a notice, returned and verified the same decision and list focus
before explicitly answering it. Both local checks passed. This is not evidence
for every possible concurrent queue mutation or terminal.

## Large queue navigation regression (2026-11-10)

`python3 tests/terminal_pty.py` passed locally with loopback access. Its large
queue case navigated decisions, notices and inbox messages containing 120,000
characters each, including vertical selection across fetch boundaries in both
directions. It also checked notice selection after earlier items were dismissed.
This is a local PTY regression, not evidence of remote behavior, consistent
pagination under arbitrary concurrent changes, or representative operator use.

## Proposed queue ordering, deferral and asynchronous update verification

The following are planned oracles only; no new implementation or test result
is claimed for these amended behaviors.

- **T-068 (deferral event):** Defer a pending decision, verify it remains
  unanswered and pending, then retrieve its deferral notification as the
  requesting agent after disconnect and service restart. Check notification
  identity/duplication and concurrent answer behavior once those contracts are
  specified.
- **T-069 (effective order):** Create requests at multiple importance levels
  and verify FIFO age within each level; swap two equal-importance positions
  and verify only their positions change and the manual order survives restart.
  Exercise a stale swap after answer, priority change, insertion and deferral.
- **T-070 (live notification):** Keep operator and agent clients connected;
  submit, defer, answer and message while recording client operations. Verify
  each subscribed client receives relevant change hints without periodic list
  requests, with bounded memory and explicit behavior when the notification
  queue is full.
- **T-071 (reconnect reconciliation):** Disconnect during several state
  changes, reconnect from a recorded cursor or perform a bounded full
  reconciliation, and verify current state is recovered despite missed,
  duplicate or reordered hints. Verify durable acceptance is decided only by
  the persisted operation response/state, never by a notification.

Transport evaluation should compare the current ZeroMQ ROUTER/DEALER design
with viable alternatives against subscription fan-out, request/reply
coexistence, bounded buffering/backpressure, reconnect behavior, missed-event
reconciliation, cross-platform support, operational dependencies and
compatibility/migration cost. A transport design must account for these criteria before replacing polling. The prior binding built bundled C++ libzmq; the current prototype uses a locally patched Rust `zeromq` crate and requires further compatibility and failure-mode validation. Replacing the crate alone does not remove application polling. `cargo tree -i cc --locked` also shows `libsqlite3-sys` as a build dependency of `cc`; eliminating the ZeroMQ native build alone does not establish a compiler-free build.

## Terminal interaction follow-up evidence (2026-11-10)

`cargo test --locked`, `python3 tests/terminal_pty.py`, `python3 tests/queue_layout.py`, `python3 tests/tmux_smoke.py`, and `python3 tests/terminal_lost_ack.py` passed locally with loopback access. They exercised explicit Enter sends, a rejected send retry, an unknown committed outcome with unchanged retry, inline custom input, scrollable help, focus and monochrome cues, and tmux including a 40 × 12 pane. The initial sandboxed Rust integration run could not create ZeroMQ sockets and aborted; the loopback-enabled rerun passed. These are local test fixtures, not representative operator, remote, or comprehensive terminal validation.

## Rust transport, mouse, and history increment (2026-11-10)

`cargo test --locked` passed all 17 unit and 23 integration tests locally with loopback access. `python3 tests/zeromq_interop.py` passed against the new Rust ROUTER using a system-libzmq DEALER; the terminal, tmux, reconnect, mouse, signal-restoration and lost-acknowledgment tests also passed. The fork's decoder unit tests passed for an oversized frame header and a multipart message with 100 frames followed by a valid message. `python3 tests/history_terminal.py` exercised multiple visible records, bounded history response navigation, and both decision and message roots. The old tmux screen-restoration test was updated to check the current framed layout and passed.

These tests do not establish a total process-memory bound, OS-crash durability, remote operation, a latency bound, or representative operator validation. A separate `/tmp` Turso 0.8.3 probe found that its resolved `simsimd` dependency requires C compilation; SQL compatibility and recovery were not tested. Per the user's direction, Aviary retains SQLite. Native libzmq remains a test-only dependency, and bundled SQLite still requires a C compiler for a fresh source build. Application-level queue refreshes and CLI waiters still poll.

## Unified queue and requester events (2026-11-10)

`python3 tests/decision_queue.py` exercised local FIFO within priority, deferral to the tail while unanswered, requester event retrieval, a restarted decision waiter, identical action retries, stale swap rejection, and persistence across service restart. A SQLite trigger forced event insertion to fail; the queue move and retry identity rolled back, and the same identity succeeded after removing the trigger. A disposable legacy database was rebuilt without the new queue tables, restarted and verified to retain old deferred records and submission identities. Terminal, tmux, reconnect, arrival, mouse, history, decision-slice, skill and CLI regressions and `cargo test --locked` passed locally. These checks do not prove arbitrary concurrent races, OS-crash behavior, a remote deployment, or representative operator acceptance.

## Event-driven updates and final local regression (2026-11-11)

`cargo test --locked` passed 18 unit and 23 integration tests. `python3 tests/change_events.py` observed no periodic reads from idle terminal, waiter and watch clients over 1.6 seconds, then exercised deferral wakeup, disconnection, reconnection and reconciliation after a service restart. `python3 tests/decision_queue.py`, `terminal_pty.py`, `terminal_reconnect.py`, `tmux_smoke.py`, `terminal_lost_ack.py`, `terminal_partial_refresh.py`, `terminal_signals.py`, `history_terminal.py`, `mouse_pty.py`, `skill_smoke.py`, and `zeromq_interop.py` passed locally with loopback access. The lost-ack proxy supplies a subscription acknowledgment itself; the separate change-events test exercises actual service subscriptions. A unit test verifies an unconfirmed deferral retains its original target and identity for a user-initiated retry even when no decision is selected.

These checks do not establish a latency guarantee, detection time for silent network partitions, a total memory bound under saturation, OS-crash durability, remote operation, or representative operator acceptance. External direct SQLite writes do not produce service change hints; durable state is reread on reconnection.

## Retained-message search API (2026-11-11)

`python3 tests/search_messages.py` passed locally: it checked text, sender, context and activity matching, literal wildcard characters, no matches, query validation, 25-result cursor traversal, bounded previews and opening a result via message history. `cargo test --locked` passed 18 unit and 23 integration tests. This verifies the API increment only; it does not verify terminal search, arbitrary concurrent mutations, search latency on a large database or remote operation.

## Terminal message search (2026-11-11)

`python3 tests/search_terminal.py` passed in a local pseudo-terminal: it searched by body, sender and context, crossed the 25-result page boundary in both directions, selected and opened a message within a shared activity, returned to the same selection, displayed an empty result, and explicitly retried a failed search after service restart. `python3 tests/search_messages.py` checked literal matching, activity, bounded previews and root-first history; `python3 tests/history_terminal.py` checked existing history behavior. `cargo test --locked` passed 18 unit and 23 integration tests. Local tests do not establish performance with large retained stores, behavior on all terminals, remote operation or representative operator acceptance.

## Operator overview (2026-11-11)

`python3 tests/overview_terminal.py` passed locally: it checked empty and populated categories, 27 retained operator messages with a multi-page inbox, decision answer and notice dismissal, event-driven count updates, service disconnection and recovery, and a forced missing-table failure showing unavailable instead of counts. `cargo test --locked` passed 18 unit and 23 integration tests; event, partial-refresh, search and terminal PTY regressions also passed. Tests use a disposable SQLite database and local pseudo-terminal. They do not establish large-database latency, remote behavior, silent-partition detection bounds or representative operator acceptance.

## Automatic local service startup (2026-11-11)

`python3 tests/auto_startup.py` passed on an isolated IPv4 loopback port with disposable directories: simultaneous terminal launches reused one process, the service retained a submission across terminal closure and service restart, a different database and unrelated listener were rejected, an explicit positional endpoint did not start a service, and an invalid database path produced a failure without replacing it. `python3 tests/startup_smoke.py`, `terminal_signals.py`, `terminal_reconnect.py`, and `zeromq_interop.py` passed; `cargo test --locked` passed 18 unit and 23 integration tests. The older startup fixture was updated to the current Decisions wording. `default_listener.py` skipped because port 7432 was occupied; that existing listener was not changed. The alternate-port test exercises the same startup path, but does not establish default-port behavior in this environment, clean-OS installation, OS-crash recovery, remote operation, daemon survival after terminal-emulator/session termination, or other supported platforms.

## Controlling-terminal hangup (2026-11-11)

`python3 tests/startup_hangup.py` passed on Linux with an isolated pseudo-terminal, controlling session and disposable loopback service. It closed the terminal master, observed the TUI process end, checked the service had its own session, and successfully submitted and retrieved a decision afterward. `python3 tests/auto_startup.py` passed after this change. The test does not establish behavior on other POSIX systems, with every terminal emulator or service manager, or across an OS crash.

## Subscription capacity and disconnection (2026-11-11)

`subscriptions_are_bounded_and_disconnects_release_capacity` opened 256 local version-2 subscriptions, confirmed an additional subscription was rejected, verified a rejected mutation produced no change hint, observed a hint after a durable submission, disconnected 128 subscribers, and observed the formerly rejected client subscribe and receive a later hint. The service remained responsive to a separate request client. This tests subscription-count limits and cleanup, not a full per-peer output queue or a total process-memory bound. The coalescing argument assumes a fresh dedicated subscription connection; the service does not enforce that restriction, so multiplexed traffic can fill its output queue with replies rather than hints. Slow-subscriber saturation, mixed-use behavior, remote partitions and latency remain open.

## Dedicated subscription enforcement (2026-11-11)

The service now requires a subscription to be the first message on a connection and ignores subsequent inbound messages on an accepted subscription, including repeated subscriptions and mutations. The local capacity test was extended to verify both directions, that an attempted answer on a subscription does not change the decision, and that a previously rejected client can subscribe using a new connection after capacity becomes available. `cargo test --locked` passed 18 unit and 24 integration tests, with local change-events, terminal reconnect, Python/libzmq and startup regressions passing. The startup fixture now chooses a separate port for its unrelated-listener case to avoid a TIME_WAIT reuse race. Normal watch, waiter and terminal clients use dedicated connections. This resolves the mixed-use limitation described in the previous entry. The tests do not force a slow subscriber's output queue to fill or prove a total memory or latency bound; remote partitions remain open.

## Preview and history resize (2026-11-11)

`python3 tests/preview_navigation.py` used a local tmux session at 100×20 and 40×12 to follow decision, inbox and notice selection, inspect decision context, and confirm no decision, message or notice was changed. `python3 tests/history_resize.py` scrolled a long history record containing CJK and combining characters, positioned a marker near the top, resized from 100×20 to 40×12 and back, and verified the marker remained visible without answering. Existing queue/detail resize checks cover additional content. `cargo test --locked`, the existing history, tmux and search regressions also passed. An older tmux help check was updated to scroll until its target rather than assuming a fixed number of help lines. These tests cover selected content and one terminal environment, not all Unicode width conventions, page mutations, platforms or representative operator use.

## Queue mutation races (2026-11-11)

`python3 tests/queue_races.py` passed on a local disposable service. Eight rounds raced competing swaps against the same versions, duplicate deferrals with the same identity, and answer against a new deferral; four further rounds raced a swap against deferral. In this run five answer-first and three defer-first outcomes occurred. It checked final order, versions, one retained event for an identical retry, correct success or rejection, and answers and events after process restart. It also inserted a new item after listing and verified a swap of two unchanged positions remained valid and persisted across restart. `python3 tests/decision_queue.py` and `cargo test --locked` passed. The service currently serializes request handling, so this is concurrent-client evidence, not proof of parallel service-instance writes, arbitrary interleavings, OS-crash durability or remote operation.
