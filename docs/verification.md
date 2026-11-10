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
