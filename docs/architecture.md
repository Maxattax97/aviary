# System architecture

**Status:** Draft architectural description for review, 2026-11-09. The stakeholder approved the initial requirements and central-service direction; this document does not approve the unresolved details or assert that V1 is implemented. The [requirements](requirements.md) define obligations, [decisions](decisions.md) records choices and open questions, and [verification](verification.md) defines the planned evidence. If this description and a requirement conflict, resolve the discrepancy rather than treating this document as a change to the requirement.

## Purpose and scope

Aviary provides durable messaging and a human attention interface for agents working across processes and, on a trusted local network, machines. Agents manage their own execution; the service manages communications; the operator makes decisions. The system is independent of any model or orchestration framework. A CLI and portable skill are the intended agent integration; MCP and a particular agent hierarchy are not required.

V1 is a single-user deployment centered on one service. It includes operator decisions and instructions, targeted agent messages and request/reply through channels, completion notices, and recoverable history. Project management, agent lifecycle control, public deployment, application authentication and multi-user isolation are outside V1. The first-slice prototype implements only a narrow decision path; see [prototype status](#first-slice-and-evidence).

## Context and boundaries

```mermaid
flowchart LR
    H[Human operator] --> T[Terminal interface]
    A[Agent or integration] --> C[CLI and portable skill]
    B[Agent on another trusted machine] --> D[CLI and portable skill]
    T -->|ZeroMQ request and reply| S[Central communication service]
    C -->|ZeroMQ request and reply| S
    D -->|ZeroMQ over trusted TCP network| S
    S --> P[(Durable local state)]
```

The diagram shows logical responsibilities, not a commitment to separate executables for every box. The service is the shared point for routing, state and recovery. The terminal interface is an independent client: closing it must not stop the service or agents. The CLI provides a documented machine-readable boundary for agents and scripts. The portable skill describes how to use that boundary; it is not an orchestrator. Local state is owned by the service, and clients use the service interface rather than treating the storage format as their contract.

One central service simplifies shared history, correlation and deduplication but is an availability and trust dependency. Agents cannot submit to it while it is unreachable. No service replication or multi-site availability is promised. The specific V1 process lifecycle, supported POSIX platforms and distribution method remain open.

## Responsibilities and interfaces

| Element | Intended responsibility | Key interfaces and constraints |
|---|---|---|
| Communication service | Accept communications, route addressed traffic, maintain request and response state, apply retry and single-decision rules, and expose retrieval and history. | A documented client interface; the service owns persistence and acknowledges acceptance only after the durability boundary. |
| Durable state | Retain accepted messages, decisions, relationships and retry identities through the agreed crash/restart boundary. | V1 default is indefinite retention without automatic deletion; retry identities last as long as their associated records. Storage loss and backup recovery are outside the agreed guarantee. |
| Terminal interface | Consolidate actionable requests; show explanations, context, priority and completion notices; support decisions, deferral and outgoing instructions. | Keyboard operated; recommended answers are first and preselected, but confirmation is explicit. Interface closure does not stop the service. |
| Agent CLI and skill | Provide a portable, documented way to submit, retrieve and wait for communications using structured results. | Caller retains the identifiers needed for retry and later retrieval. No mandatory MCP, runtime or model. |
| Agent and operator | Supply request content, context and routing labels; agents act on responses, and the operator controls importance and makes decisions. | Agent suggestions do not make decisions for the operator. Names are routing labels, not authenticated identities. |

The selected communication transport is ZeroMQ over TCP. The precise application schema, versioning, message limits, delivery acknowledgments, client compatibility and service lifecycle require further design. The current prototype protocol is described in the [README](../README.md); it is not a stable V1 contract.

## ZeroMQ communication design

The target architecture uses ZeroMQ between each CLI or terminal client and the central service. Use a service-side ROUTER socket and client-side DEALER sockets for request/reply, allowing multiple clients and multiple outstanding operations without coupling service processing to strict alternating request/reply. This socket arrangement is the initial design to validate during implementation; the Rust ZeroMQ implementation, Tokio integration and exact multipart envelope remain open. A client request carries an application correlation identifier; the service replies to the originating connection with that identifier and an explicit success or error. A ZeroMQ routing identity is transient transport metadata, not an authenticated participant or a durable submission identity.

Use the operator-selected interface as the ZeroMQ TCP bind endpoint, with IPv4 loopback as the default. Clients connect to that endpoint; they do not need a separate ZeroMQ broker. Continue to expose a documented CLI with machine-readable output so integrations need not speak ZeroMQ directly. Version the application protocol independently of the transport; define framing, size limits, timeouts and compatibility before declaring it stable. Switching transports breaks direct compatibility with the existing raw-TCP prototype; coordinated client and service updates are required. The persisted database and the CLI's documented operations should remain compatible unless a migration is explicitly specified and tested.

ZeroMQ provides asynchronous messaging, routing and reconnection mechanisms, not Aviary's durable acceptance or delivery guarantees. The service must persist before returning an application-level acceptance response. A successful ZeroMQ send or queue insertion is not proof of service acceptance. On a timeout or disconnect, a caller must treat the outcome as unknown and retry submissions using the same stable identity; the service returns the original identity for identical content and rejects conflicts. Queue limits, backpressure, unavailable peers and shutdown behavior require explicit bounded policies and tests; a transport reconnect alone does not establish delivery. Keep database-backed retrieval as the recovery path. If notifications are introduced, use them only as hints and reconcile from durable state: subscribers can miss messages or disconnect. See the [ZeroMQ socket API](https://zeromq.org/socket-api/) and [reliability guide](https://zguide.zeromq.org/docs/chapter4/).

## Communication and state

The V1 domain distinguishes participants, channels, addressed communications, decision requests, responses and activity references. A channel supports one or more senders and recipients; messages can target an individual or group. An activity reference relates exchanges without establishing a managed project hierarchy. A request has a correlated response; the V1 agent-to-agent scope is a targeted request and one response. Membership administration, recipient selection, group response races and the timing of membership evaluation remain open. Routing a message only to matching labels is functional delivery behavior, not authorization against a process that can access the service.

A decision starts pending. A recommendation is presentation guidance: it becomes an answer only after explicit operator confirmation. A pending request may be deferred and later retrieved; deferral is not a response. On acceptance of an answer, the service records one decision and makes its structured representation retrievable by the requester. A second decision must be rejected, including under concurrent attempts. The exact response retry and concurrency contract needs design and verification; no cancellation or rollback of an action already taken is implied.

The operator queue is a view of actionable work, ordered by importance with operator override. Its actionable classification, importance scale, tie-breaking, notice lifecycle and deferral policy are open decisions. The queue is not the authoritative durable store. Completion notices carry the work and result supplied by the agent. History associates related messages, decisions and outcomes; ordering and the detailed relationship model remain open.

## Principal flows

1. **Submit and decide.** An agent supplies a stable submission identity, question, work context and any suggested answers or recommendation. The service validates and durably records an accepted request before acknowledging it. The terminal retrieves pending work, presents the recommendation first when present, and sends an answer only upon explicit confirmation. The service records one decision; the requester waits for or retrieves the correlated structured answer. Custom answers and requests with no suggestions are V1 requirements, not first-slice capabilities.
2. **Retry and recover.** If the acknowledgement is lost, the caller may retry with the same deployment-scoped submission identity and content. The service returns the existing request identity for an identical retry and rejects different content under the same identity. Following an acknowledged commit, a service or OS crash and restart with intact storage must preserve the accepted record under the agreed storage assumptions. A requester retaining its request identifier can retrieve a response after its own restart. Recovery from a lost storage device is not included.
3. **Route and reconnect.** A sender addresses a communication through a channel to an agent or group. The service records an accepted communication and makes it retrievable according to the eventual routing and delivery policy. An agent that disconnects can retrieve relevant communications accepted for it during the interruption after reconnecting. This does not mean an unreachable service accepted a submission, and the acknowledgment and membership semantics still need definition.
4. **Operate independently.** The service continues running while the terminal interface is closed. An operator can reopen the interface and retrieve pending work. The startup path must not require manually orchestrating multiple background processes; its concrete lifecycle and installation approach remain open.

## Deployment, trust and failure boundaries

By default the listener binds to localhost. An operator can explicitly select a network interface for agents on a trusted local cluster. V1 has no application authentication or authorization: **any process able to reach the listener is trusted**, including local processes. It can impersonate routing labels or access work through the available interface. The intended V1 ZeroMQ deployment does not add application authentication or transport encryption; the current first-slice protocol is likewise unencrypted. Do not expose this listener to an untrusted network. Future authentication, per-communication authorization and multi-user isolation require a separate security design and threat model; they are not properties of channel membership.

Accepted means positively acknowledged after the durable commit. The approved failure boundary covers service or OS crash and restart with the storage device intact, subject to a functioning storage stack; loss of the device and backup recovery are excluded. Network interruption can prevent submission or response retrieval until connectivity returns. A client must retain its retry identity or request identifier as appropriate; the system cannot recover an identifier that a caller has lost. Capacity limits, filesystem assumptions, timeouts and operational alerting need further decisions. Verification must distinguish process termination from an OS crash, power loss or device failure.

## First slice and evidence

The current Rust prototype uses ZeroMQ ROUTER/DEALER over TCP through libzmq, Tokio, and SQLx with SQLite WAL and `synchronous=FULL`. It uses typed protocol operations, typed errors and tracing for service logs. It binds `127.0.0.1:7432` by default and supports an explicit bind address. A single service executable handles submit, list, answer and get; separate CLI and terminal invocations connect to it. The terminal polls, displays the oldest pending request and allows keyboard selection of a suggestion or custom answer with explicit confirmation. A requester can poll for the correlated answer. The prototype accepts any number of suggestions and an optional recommendation and supports initial immutable channels and recipient-snapshot message inboxes. Agent request/reply and operator message composition remain unimplemented. Its 1 MiB protocol message limit and 500 ms polling interval are prototype choices, not approved V1 limits or latency guarantees.

The local integration test exercises acknowledged submission, service process kill/restart, identical and conflicting retries, one answer followed by rejection of a second, concurrent identical submissions and competing answers, and retrieval after restart. This is partial evidence, not full acceptance of the corresponding requirements. The initial ZeroMQ transport has local process-kill and concurrency coverage; transport failure tests, an interactive terminal check, two-machine test, full concurrent-answer acceptance, OS-crash evidence and representative operator validation remain outstanding. See [tickets](tickets.md) and [verification](verification.md) for the work and test plan.

## Architecture-to-verification map

The following map identifies principal architectural responsibilities and planned verification; it does not replace the full [traceability matrix](traceability.md) or imply that a test has passed.

| Concern | Principal requirements and planned tests | Backlog |
|---|---|---|
| Attention, explanation, context and priority | SYS-001–005, SYS-017–021, SYS-038; T-001–005, T-017–021, T-038 | V1-01–04, V1-11–13 |
| Decision presentation and structured response | SYS-006–011, SYS-036; T-006–011, T-036 | V1-05–06, V1-16 |
| Channels, targeted messages and correlated responses | SYS-012–016, SYS-040; T-012–016, T-040 | V1-07–10 |
| Agent integration, terminal operation and lifecycle | SYS-022–024, SYS-028–029, SYS-039; T-022–024, T-028–029, T-039 | V1-12, V1-14, V1-18, V1-22 |
| Durability, history, identity and recovery | SYS-025–027, SYS-033, SYS-041–042; T-025–027, T-033, T-041–042 | V1-15, V1-17, V1-21 |
| Distribution, context and listener boundaries | SYS-030–032, SYS-043–044; T-030–032, T-043–044 | V1-19–20, SL-05 |
| Future authentication, authorization and user isolation | SYS-034–035, SYS-037; T-034–035, T-037 | L-13–15; future security design |

## Decisions needed before completing V1

The authoritative open-question list is in [decisions](decisions.md). In particular, the channel and delivery model, attention and deferral policy, response races and retry semantics, ZeroMQ implementation and integration details, operational limits, protocol compatibility, lifecycle, and platform/terminal support matrix need decisions and verification before the relevant tickets can be completed. Review changes against the requirements, update the decisions and this architecture together, and retain explicit evidence and limitations in the verification records.
