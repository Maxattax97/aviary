# Open product and engineering decisions

See [architecture](architecture.md) for the system context, flows and component responsibilities.

**Status:** Initial requirements and architecture direction approved 2026-11-09. The remaining decisions below are open; the first-slice choices are recorded separately below.

1. **Attention policy:** The operator controls importance; agent suggestions and operator overrides suffice in V1. The initial importance and deferral choices are recorded below; actionable classification, completion-notice visibility and notice lifecycle remain open. How are oversized summaries and context handled? US-008 resurfacing is V2.
2. **Request/reply scope:** V1 includes a targeted agent-to-agent request and one correlated response through channels. Channels support one or many senders and recipients; clarify membership, addressed subsets, timeouts, concurrent responders and group reply semantics. US-026 remains V2 and US-044 remains Future; richer autonomous coordination is not a V1 commitment.
3. **Recovery and history:** Acknowledge after durable commit; identical retries return the existing request and retries with conflicting content fail. V1 defaults: retain pending and completed items indefinitely with no automatic deletion; retain retry identities while their associated records exist, scoped to the single deployment; reject conflicting retry content. Configurable retention is V2. Deletion and related-history behavior can be designed with V2 retention controls. A durable commit covers service or OS crash and restart when the storage device remains intact; device loss and backup recovery are outside that guarantee. Define delivery/acknowledgment semantics and history ordering. US-023 covers requester restart; US-053 covers reconnect after accepted messages; Future US-047 has broader offline delivery intent. US-038/051 provide linked request history and activity references; US-052’s cross-session timeline is V2.
4. **Trust and deployment:** Use one single-user service, bound to localhost by default, with an explicit network-interface override for trusted clusters and no V1 application authentication. Use ZeroMQ for client/service communication; choose the Rust implementation, integration details, storage design beyond the prototype and service lifecycle. Future US-045, US-054 and US-055 require a separate security and isolation design.
5. **Interaction and compatibility:** Target POSIX systems; define the concrete supported platforms, terminals and minimum dimensions; key bindings, accessibility, no-recommendation behavior, payload limits, machine-readable schema/versioning, update latency and setup prerequisites. Confirm the measurable one-confirmation workflow and operational limits.
6. **Future behaviors:** US-017 pending request revision needs a design for edit authority and visibility. US-018 remains V2: the agreed direction is a linked correction message preserving the original and reporting delivery, contingent on feasibility and cost and only where correction can still influence the recipient; delivery does not prove the recipient has not acted, and neither rollback nor cancellation is guaranteed. US-041 concerns identifying noisy sources, not automatic suppression. US-050 environment navigation and optional tmux/Herder integration require availability and safety decisions.

## Initial architecture hypothesis and tradeoffs

A durable communication service, documented machine-readable CLI, and independent terminal client are a plausible minimal boundary. A central service is selected: it simplifies isolation, consistent history and deduplication, but introduces an availability and trust dependency. Future distributed deployment would only be justified by a demonstrated need such as multi-site availability or data locality; it is not a V1 goal. Local storage can simplify deployment but requires explicit durability and backup assumptions. Transport and credentials affect deployability and security. ZeroMQ is selected for the target transport; its implementation details, storage design beyond the prototype and lifecycle remain open. Avoid general-purpose chat, project management, agent lifecycle control, mandatory MCP or a mandatory agent hierarchy.

## Story analysis for review

- **Ambiguity:** “Actionable,” “importance,” “meaningful,” “minimal setup,” “reasonable number,” and “temporarily” lack measurable thresholds. The proposed requirements preserve the observable core; the decisions above must resolve operational limits.
- **Potential conflicts:** Default preselection (US-013) could be mistaken for automatic approval; explicit confirmation preserves the human decision. US-054 network authorization is Future; V1 explicitly permits unauthenticated access on an operator-selected trusted network. The baseline contains no proven logical contradiction, but the precise boundary between V1 request/reply and later coordination still needs definition.
- **Missing behavior:** Identity provisioning, membership changes, response races, failed submission reporting, retry conflicts, retention, and network or storage failure boundaries need decisions before a complete implementable contract. The proposed single-decision rule resolves ambiguity for the vertical slice and needs review; it does not confer rollback or correction.
- **Overlap without deletion:** US-022/026/044 concern related request/reply at different scopes or priorities; US-023/047/053 cover restart, broader offline delivery, and reconnection respectively. US-038/051/052 cover request history, activity association, and a later cross-session timeline. US-041 is source visibility, not a mandate to suppress messages.
- **Unstated constraints:** The baseline does not establish an availability target, supported platforms, payload size, or recovery from loss of the storage device. Do not claim these have been met until the human approves measurable bounds.

## V1 local deployment and future isolation

Run a single-user service bound to localhost by default, without application-level authentication. The operator may explicitly bind a network interface for a trusted local cluster. Treat every process that can reach the endpoint as trusted: agent names and channel memberships route messages but cannot prevent an accessible process from impersonating a participant. Do not expose the unauthenticated listener to an untrusted network. Network authentication and authorization are Future.

All participants within the single user’s context may create channels. Future multi-user support must isolate users and their channels; account creation, registration, authentication and sharing mechanisms remain undecided, with no assumption of a username/password system or trusted administrator. Do not claim V1 supplies that isolation. A future public service requires its own threat model and security design.

Use Vim-style navigation (`j`/`k`), Enter to confirm, and numbered options beginning at 1; the recommendation remains preselected. Use a separate selection mechanism for more than nine options so the number of choices is not capped. Reserve `q` for leaving a view rather than overloading it as an answer.

## First-slice engineering choices

The initial first-slice prototype used newline-delimited JSON over raw TCP; the current transport is described below. It uses a separate terminal client and SQLite in WAL mode with FULL synchronous commits. Bind IPv4 loopback by default and accept an explicit bind address. A submission identity is unique across the deployment and retained with its record. Compare the complete normalized request for retry conflicts. The initial slice required options and a recommendation. It now accepts zero or more nonempty suggestions; the recommendation and nonempty reason must be supplied together, and the index must be valid. It displays an importance-ordered decision queue with a separate deferred view. A recommendation is preselected; without one, no choice is initially selected. The operator may navigate suggestions or the last custom choice and must confirm explicitly. Poll for updates and responses every 500 ms without a promised latency bound. Retain all records indefinitely. A second answer is rejected; no answer retry identity is defined yet. The original raw-TCP prototype limited a protocol message to 1 MiB including its newline. The process-crash durability test does not establish storage-device or power-loss guarantees. The full channel model, interaction policy, and supported platform matrix remain open.

## ZeroMQ transport decision

The user selected ZeroMQ for the target client/service communication on 2026-11-09. The initial architecture uses DEALER clients and a ROUTER service over TCP; initial implementation choices are recorded below; backpressure and interoperability require further verification. A ZeroMQ send is not a durable acknowledgment: persistence, retry deduplication, response correlation, recovery and the single-decision rule remain service responsibilities. Live notifications, if used, require reconciliation with durable state. V1 still has no application authentication or authorization and must be limited to trusted networks; the transport selection does not imply encryption. The initial migration is implemented; verification is incomplete.


## ZeroMQ first implementation choices

The first migration increment uses the `zmq` Rust binding to libzmq on a Tokio blocking worker; the service processes requests serially. A DEALER sends one JSON frame with version 1, a nonempty correlation string of at most 128 bytes, and an operation; the ROUTER returns one frame with the version, correlation, and result. The routing identity is transport metadata. Each current CLI call uses a separate socket and one outstanding correlation. Reject unsupported versions and over-limit application frames; malformed multipart/envelope input may be dropped without a reply. Use 1 MiB per frame, high-water marks of 64 messages per peer, 10-second send/receive timeouts, and zero linger. A timeout is an unknown outcome and submissions require the same retry identity. These are initial operational choices, not complete resource isolation: libzmq's frame limit does not bound aggregate multipart input or the number of peers. Backpressure, lost replies, shutdown, interoperability and cross-machine behavior remain to be verified.

A custom answer is a nonempty string submitted through `answer_custom`; its exact text is persisted as a structured `custom` value. Suggested and custom answers compete for the same single-decision conditional write. The terminal provides single-line custom composition. Wider terminal and representative-operator verification remain outstanding.


## Initial channel and delivery choices

A channel has an immutable name and nonempty set of participant labels; identical creation is idempotent, while a different membership under the name conflicts. Any member can send to another named member or to the group, which snapshots every member except the sender as recipients at acceptance. A group with no recipient is invalid. An individual may address itself. Inbox retrieval is non-destructive, ordered by message ID, and retains the accepted message and its recipient snapshot indefinitely. No read receipt or destructive acknowledgment is implied. Message and decision retry keys share the deployment-wide submission namespace; the transaction commits the message, retry key and recipient snapshot together. Context and activity are optional caller-supplied strings. This routing is not authentication or authorization: any reachable process can impersonate a label or retrieve another label's inbox. Changes to membership, pagination, request/reply and operator composition remain open.


## Initial agent request/reply choice

A directly addressed message can set `expects_reply`; group requests are rejected to avoid undefined group response races. The target label supplies one response with its own deployment-scoped retry identity. One transaction persists the response message, the request-to-response link, the recipient snapshot and retry key. Identical retries return the original response ID; a different later response is rejected. The response is delivered to the original sender's inbox and carries the original context and activity. `get_message_response` and `wait-message` use the retained request message ID. A waiter polls indefinitely until an answer or service error; restarting it with the ID retrieves an existing answer. Sender labels are not authenticated.


## Decision queue priority and deferral

A decision submission may specify an integer importance from 0 through 5, default 0; higher values sort first and equal values sort by request ID (submission order). The operator may persist an override without changing the submitted content or its retry identity. Deferral moves a pending decision to a separate durable queue until the operator restores or answers it; it is not a response and does not resurface automatically. The interface exposes h/l request navigation, d for deferral or restoration, Tab to switch queues and p then 0–5 for an override. Classification of notices, large-content display, and broader accessibility remain open.


## Decision explanation and context

Decision requests may carry optional explanation and supporting-context strings. The queue abbreviates question, agent, explanation and answer previews to fit an estimated single line, with an ellipsis when truncated; c opens the full agent, question, explanation and context, and o opens the selected answer and its recommendation reason. Details use pages of at most 200 characters sized conservatively for the current terminal dimensions. Control characters are rendered as spaces to prevent terminal injection; stored text is unchanged. The transport frame limit applies, and terminal-size-aware layout and queue pagination remain open.


## Completion notice lifecycle

An explicit `submit_notice` represents a meaningful completion and requires agent, work and result text; routine messages do not become notices. It uses the deployment-scoped retry identity. Open notices are indicated by count in the decision interface and viewed separately with n. An operator dismisses a notice with d; it remains stored and can be listed with dismissed entries. There is no automatic deletion or resurfacing. Work, result and optional activity are displayed in pages of at most 200 characters sized conservatively for the current terminal dimensions with control characters rendered as spaces. Full queue pagination and broader operator validation remain open.


## Operator instruction composition

The terminal uses the routing label `operator`, which must be a member of the selected immutable channel. The operator supplies the deployment-unique retry identity, channel, recipient label or group, and single-line instruction text, then explicitly confirms the review. Long edit fields show a trailing preview, and the complete review is available in pages navigated with j/k. A failed or unknown send retains the unchanged draft for retry with the same identity while the interface remains open; drafts are not durable, so the operator must retain the identity and content before closing or cancelling. This label is not authenticated.


## Request history

A decision or channel message can be the history root. Related decisions, messages and completion outcomes share an explicit nonempty activity reference; direct agent request/response links are also included even when no activity is supplied. A single database read transaction supplies a consistent current snapshot. Decisions, messages and outcomes are returned as separate arrays ordered by their own record IDs; this is not a cross-kind chronological timeline. Answer values and dismissed notices remain visible. Records without a shared activity are not inferred as related. Activity references are caller-supplied labels, not access controls. History can also be read section by section using `history_page`; a cross-session timeline remains open.


## Agent skill distribution and compatibility

The portable on-demand skill is maintained in `skills/aviary/` and can be copied or supplied directly to an agent without MCP. Its examples use the installed Aviary CLI and Python 3 JSON serialization. The version-1 transport envelope and CLI operation schemas are provisional; deploy service, CLI and skill as a matched version until a stable compatibility policy is established. A caller must preserve submission identity and content for unknown outcomes.


## Initial queue wire paging

The terminal retrieves decision and notice queues in offset pages targeted at 900 KiB, with a larger single item allowed within a reserved frame budget. Legacy list operations remain unchanged. Each page is evaluated against current state, not a snapshot, so concurrent queue changes can shift offsets; later refreshes reconcile the view. A single near-frame-limit stored item can still exceed the page budget. Inbox and each history section also offer offset pages; history pages include the root ID and section, while the unpaged history response includes root and activity metadata. Pages are independently evaluated against current state and do not share a snapshot. Decision, notice and inbox page operations read rows incrementally and stop after a page plus one lookahead item. History pages also read the selected section incrementally. SQLite may need temporary storage for ordering, and terminal aggregation is not memory-bounded. These limitations remain open for V1 readiness.


## Service signal shutdown

On POSIX, SIGINT and SIGTERM request service shutdown. Once the signal is observed, the service stops accepting further operations and waits for a currently executing operation and its reply attempt before closing the zero-linger socket. Queued requests or replies can be lost; clients treat missing replies as unknown outcomes and retry identical submissions with the same identity. Idle receive polls at 100 ms; there is no guaranteed overall shutdown deadline because storage and reply handling may take longer. This does not replace a supervisor, backup, or crash recovery.


## Answer choice viewport

The decision view estimates the available choice rows from terminal height, keeps the selected choice in that window, and redraws on resize. The custom answer is included as the last choice. This is an initial row-count approximation: wrapped question, explanation, option and help text can still consume extra rows, and very small panes are not supported by an established minimum. Full terminal-size-aware layout remains open.


## Detail page sizing

Decision context and notice detail pages use the terminal dimensions to estimate available space, reserving rows for headings and navigation and allowing two columns per character, capped at 200 characters per page. Resize recalculates page boundaries; the current page index is clamped if needed, so a reader may need to navigate again after a resize. This approximation does not establish a supported minimum size or handle every possible Unicode width and wrapped header.


## Decision previews

Decision queue previews use a conservative two-column-per-character estimate; a truncated recommendation places its label before the answer text. `c` opens the full request detail and `o` opens the full selected suggested answer and recommendation reason. Confirming still requires Enter; opening a detail does not submit. Header, footer and edit-field wrapping, very small panes, and exact display widths for all Unicode remain open.


## Instruction review paging

The instruction composer retains complete input while showing the end of long fields. Its confirmation view pages all fields using the same terminal-size estimate as other detail views; j/k navigates pages and Enter explicitly sends the unchanged draft from any page. A failure retains the draft and identity while the interface stays open. Resize can change page boundaries; full layout and usability across supported terminals remain to be verified.


## Custom answer review

A long custom answer shows its tail while being edited. Ctrl-R opens a paged review of the full draft; j/k navigates, and q or Esc returns to editing. Enter in the editor still submits the exact draft, preserving the established confirmation flow. The draft is not durable across interface exit. Page sizing and terminal limitations are the same as other detail views.


## Terminal recovery from unavailable service

The terminal retains its current view and unsent custom draft when polling fails, displays the error and retries reads; a later successful poll reconciles the view. A failed action is shown as not confirmed, retains the current state, and is never automatically resubmitted. A missing reply can conceal a committed action; check current state before a manual retry. Polling calls use one-second send and receive timeouts per call; queue refreshes run outside the input loop, with one refresh at a time and stale results discarded after queue switches and actions. Submissions run in a separate task; while pending, the interface ignores additional actions and allows Ctrl-C to request exit. Closing the interface still discards unsent drafts.


## Background terminal refresh

At most one queue-and-notice refresh runs at a time outside the input event loop. A generation check discards a refresh started before a queue switch or state-changing action; a refresh completing while an instruction is being composed is also discarded. An interrupted refresh is reconciled by a later poll. Only one submission may be outstanding; completion reconciles the view. Ctrl-C requests exit, but process shutdown may wait for an outstanding blocking transport call to time out.


## Pending terminal actions

A decision, attention change, notice dismissal or instruction send runs outside the input loop. While awaiting the response, the terminal indicates that the outcome is unknown and ignores additional actions; resize and Ctrl-C remain available. A prior error is cleared when an explicit retry starts. Once an action completes, the existing success or unconfirmed-outcome behavior applies, and reads reconcile the view. Exiting does not cancel a potentially committed operation, and runtime shutdown may wait for the transport timeout.


## Incremental queue and inbox pages

Decision, notice and inbox page operations read ordered database rows incrementally from the requested offset and stop after a page plus one lookahead. Decision ordering is the effective importance descending and ID ascending, matching the unpaged operation; missing stored importance defaults to zero. Offset pages are independently evaluated and can shift during mutations. This bounds the application-level page collection, not all SQLx, SQLite, process or terminal memory: SQLite may sort or scan many rows, unpaged operations still materialize results, and the terminal collects every fetched page.


## Incremental history pages

A history page resolves its root and reads only the requested section in a single SQLite read transaction, stopping after a page plus one lookahead item. Related activity and direct request/reply linkage retain the same per-section ID order as unpaged history. Each page is a separate snapshot, so changes between requests can shift offset traversal. SQL scan and temporary-storage costs, total process memory, unpaged history and cross-kind chronological ordering remain outside this change.


## Operator inbox

The terminal can read the non-destructive inbox for the `operator` routing label. It loads and refreshes inbox pages while that view is open, retains selection by message ID, and pages sender, work context, body and activity for inspection. `i` opens or closes the inbox, h/l moves between messages, and j/k moves through content. There is no unread state, removal, or correlated reply action in this view; use the CLI for reply and history. Routing labels remain unauthenticated. Inbox aggregation is not memory-bounded, and representative operator and remote-machine validation remain open.
