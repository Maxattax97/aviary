# Open product and engineering decisions

See [architecture](architecture.md) for the system context, flows and component responsibilities.

**Status:** Initial requirements and architecture direction approved 2026-11-09. The remaining decisions below are open; the first-slice choices are recorded separately below.

1. **Attention policy:** The operator controls importance; agent suggestions and operator overrides suffice in V1. The initial importance, deferral and explicit attention classification choices are recorded below; their quality in real operator use remains to be validated. How are oversized summaries and context handled? US-008 resurfacing is V2.
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

The first migration increment uses the `zmq` Rust binding to libzmq on a Tokio blocking worker; the service processes requests serially. Initially a DEALER sent one JSON frame with version 1, a nonempty correlation string of at most 128 bytes, and an operation; the ROUTER returns one frame with the version, correlation, and result. The routing identity is transport metadata. Each current CLI call uses a separate socket and one outstanding correlation. Reject unsupported versions and over-limit application frames; malformed multipart/envelope input may be dropped without a reply. The initial frame limit was 1 MiB; see the later version-2 response decision. High-water marks are 64 messages per peer, with 10-second send/receive timeouts and zero linger. A timeout is an unknown outcome and submissions require the same retry identity. These are initial operational choices, not complete resource isolation: libzmq's frame limit does not bound aggregate multipart input or the number of peers. Backpressure, lost replies, shutdown, interoperability and cross-machine behavior remain to be verified.

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

The following records the current implementation; the planned default below supersedes its manual-field workflow for future implementation.

The terminal uses the routing label `operator`, which must be a member of the selected immutable channel. The operator supplies the deployment-unique retry identity, channel, recipient label or group, and single-line instruction text, then explicitly confirms the review. Long edit fields show a trailing preview, and the complete review is available in pages navigated with j/k. A failed or unknown send retains the unchanged draft for retry with the same identity while the interface remains open; drafts are not durable, so the operator must retain the identity and content before closing or cancelling. This label is not authenticated.


## Request history

A decision or channel message can be the history root. Related decisions, messages and completion outcomes share an explicit nonempty activity reference; direct agent request/response links are also included even when no activity is supplied. A single database read transaction supplies a consistent current snapshot. Decisions, messages and outcomes are returned as separate arrays ordered by their own record IDs; this is not a cross-kind chronological timeline. Answer values and dismissed notices remain visible. Records without a shared activity are not inferred as related. Activity references are caller-supplied labels, not access controls. History can also be read section by section using `history_page`; a cross-session timeline remains open.


## Agent skill distribution and compatibility

The portable on-demand skill is maintained in `skills/aviary/` and can be copied or supplied directly to an agent without MCP. Its examples use the installed Aviary CLI and Python 3 JSON serialization. The transport envelopes and CLI operation schemas are provisional; deploy service, CLI and skill as a matched version until a stable compatibility policy is established. A caller must preserve submission identity and content for unknown outcomes.


## Initial queue wire paging

The terminal retrieves decision and notice queues in offset pages targeted at 900 KiB, with a larger single item allowed within a reserved frame budget. Legacy list operations remain unchanged. Each page is evaluated against current state, not a snapshot, so concurrent queue changes can shift offsets; later refreshes reconcile the view. At that stage a single near-frame-limit stored item could exceed the page budget; see the later version-2 response limit. Inbox and each history section also offer offset pages; history pages include the root ID and section, while the unpaged history response includes root and activity metadata. Pages are independently evaluated against current state and do not share a snapshot. Decision, notice and inbox page operations read rows incrementally and stop after a page plus one lookahead item. History pages also read the selected section incrementally. SQLite may need temporary storage for ordering, and at that stage terminal aggregation was not memory-bounded; see the later terminal paging decisions. These limitations remain open for V1 readiness.


## Service signal shutdown

On POSIX, SIGINT and SIGTERM request service shutdown. Once the signal is observed, the service stops accepting further operations and waits for a currently executing operation and its reply attempt before closing the zero-linger socket. Queued requests or replies can be lost; clients treat missing replies as unknown outcomes and retry identical submissions with the same identity. Idle receive polls at 100 ms; there is no guaranteed overall shutdown deadline because storage and reply handling may take longer. This does not replace a supervisor, backup, or crash recovery.


## Answer choice viewport

The decision view estimates the available choice rows from terminal height, keeps the selected choice in that window, and redraws on resize. The custom answer is included as the last choice. This is an initial row-count approximation: wrapped question, explanation, option and help text can still consume extra rows, and very small panes are not supported by an established minimum. Full terminal-size-aware layout remains open.

When the full recommendation reason does not fit the preview, the option uses a shorter `[rec]` marker so that some answer text remains visible; `o` opens its complete text and reason. On terminals narrower than 60 columns, the decision view uses two shorter help lines; the README lists all keys. This reduces wrapping for the tested 40×12 pane but does not establish a minimum size or guarantee every header and Unicode string fits.


## Detail page sizing

Decision context and notice detail pages use the terminal dimensions to estimate available space, reserving rows for headings and navigation and allowing two columns per character, capped at 200 characters per page. On resize, the interface maps the start of the prior page into the new page boundaries. This can repeat earlier text, and the rest of the old page may move to a later page. The same mapping applies to custom-answer review, instruction and reply review, inbox and history text. This approximation does not establish a supported minimum size or handle every possible Unicode width and wrapped header.


## Decision previews

Decision queue previews use a conservative two-column-per-character estimate; a truncated recommendation places its label before the answer text. `c` opens the full request detail and `o` opens the full selected suggested answer and recommendation reason. Confirming still requires Enter; opening a detail does not submit. Header, footer and edit-field wrapping, very small panes, and exact display widths for all Unicode remain open.


## Instruction review paging

The instruction composer retains complete input while showing the end of long fields. Its confirmation view pages all fields using the same terminal-size estimate as other detail views; j/k navigates pages and Enter explicitly sends the unchanged draft from any page. A failure retains the draft and identity while the interface stays open. Resize can change page boundaries; full layout and usability across supported terminals remain to be verified.


## Custom answer review

A long custom answer shows its tail while being edited. Ctrl-R opens a paged review of the full draft; j/k navigates, and q or Esc returns to editing. Enter in the editor still submits the exact draft, preserving the established confirmation flow. The draft is not durable across interface exit. Page sizing and terminal limitations are the same as other detail views.


## Terminal recovery from unavailable service

The terminal retains its current view and unsent custom draft when polling fails, displays the error and retries reads; a later successful poll reconciles the view. A failed action is shown as not confirmed, retains the current state, and is never automatically resubmitted. A missing reply can conceal a committed action; check current state before a manual retry. Polling calls use one-second send and receive timeouts per call; queue refreshes run outside the input loop, with one refresh at a time and stale results discarded after queue switches and actions. Submissions run in a separate task; while pending, the interface ignores additional actions and allows Ctrl-C to request exit. Closing the interface still discards unsent drafts.

Decision, notice and open-inbox refreshes are independent: a failed page is identified in the refresh warning while successful pages update normally. Failed views retain their prior contents until a successful read; those contents may be stale. Each read is a separate request and snapshot, so this does not provide a cross-view consistent snapshot or a latency guarantee.

In panes narrower than 60 columns the warning names the affected views without the full error text; wider panes show the detailed errors. The compact decision heading and help reserve space for that warning. This is an initial layout for narrow panes, not a guarantee for all dimensions or content.

While a custom answer is being edited, a refresh whose current page no longer contains its request retains the displayed request and draft and warns the operator. The request may have been answered or deferred elsewhere; submission remains subject to current service state. Leaving the editor permits the next refresh to reconcile the queue and discard the draft if the request is absent. Drafts are not persisted across interface exit.


## Background terminal refresh

At most one queue-and-notice refresh runs at a time outside the input event loop. A generation check discards a refresh started before a queue switch or state-changing action; a refresh completing while an instruction is being composed is also discarded. An interrupted refresh is reconciled by a later poll. Only one submission may be outstanding; completion reconciles the view. Ctrl-C requests exit, but process shutdown may wait for an outstanding blocking transport call to time out.


## Pending terminal actions

A decision, attention change, notice dismissal or instruction send runs outside the input loop. While awaiting the response, the terminal indicates that the outcome is unknown and ignores additional actions; resize and Ctrl-C remain available. A prior error is cleared when an explicit retry starts. Once an action completes, the existing success or unconfirmed-outcome behavior applies, and reads reconcile the view. Exiting does not cancel a potentially committed operation, and runtime shutdown may wait for the transport timeout.

The terminal also handles external SIGINT and SIGTERM by returning through its terminal cleanup guard. A pending operation can still have committed, and unsent drafts are discarded. SIGKILL, a crash and loss of the terminal cannot run that guard and are not covered by this behavior.


## Incremental queue and inbox pages

Decision, notice and inbox page operations read ordered database rows incrementally from the requested offset and stop after a page plus one lookahead. Decision ordering is the effective importance descending and ID ascending, matching the unpaged operation; missing stored importance defaults to zero. Offset pages are independently evaluated and can shift during mutations. This bounds the application-level page collection, not all SQLx, SQLite, process or terminal memory: SQLite may sort or scan many rows, unpaged operations still materialize results, and at that stage the terminal collected every fetched page; see the later terminal paging decisions.


## Incremental history pages

A history page resolves its root and reads only the requested section in a single SQLite read transaction, stopping after a page plus one lookahead item. Related activity and direct request/reply linkage retain the same per-section ID order as unpaged history. Each page is a separate snapshot, so changes between requests can shift offset traversal. SQL scan and temporary-storage costs, total process memory, unpaged history and cross-kind chronological ordering remain outside this change.


## Operator inbox

The terminal can read the non-destructive inbox for the `operator` routing label. It loads and refreshes inbox pages while that view is open, retains selection by message ID, and pages sender, work context, body and activity for inspection. `i` opens or closes the inbox, h/l moves between messages, and j/k moves through content. There is no unread state or removal; use the CLI for history. A direct message that requests a reply offers r to compose a correlated response. Routing labels remain unauthenticated. The inbox retains one response page and an offset history for backward navigation; representative operator and remote-machine validation remain open.


## Operator inbox replies

For a direct message addressed to `operator` with `expects_reply`, r opens a reply draft tied to that message ID. The operator provides a deployment-unique retry identity and body, reviews the intended recipient and request ID, and explicitly confirms. Identical retries return the same reply ID; a different second reply is rejected by the service. A failed or uncertain send retains the draft and identity until the interface is closed or the operator cancels. The inbox does not indicate whether a request was already answered. Labels remain unauthenticated.


## Malformed multipart handling

The service expects exactly a ROUTER identity and one payload frame. It discards extra frames individually before accepting another message instead of collecting the multipart payload in an application vector; shutdown can interrupt draining. The CLI rejects a response with extra frames. This limits application-side copying for malformed envelopes, not aggregate libzmq buffering, peer count, total process memory, CPU consumed draining a multipart message, or fairness under saturation. Trusted-network deployment and the existing unknown-outcome rules still apply.


## Slow peer reply backpressure

The ROUTER uses mandatory routing and nonblocking reply sends. If a recipient is disconnected or its per-peer reply queue is full, the service does not wait for that peer; it logs the failure at debug level and continues. The operation may already have committed, and a missing response remains an unknown outcome requiring the appropriate retry or retrieval procedure. This does not guarantee delivery, bound aggregate libzmq buffering, or ensure fairness under arbitrary request load: operations and malformed-message draining remain serial.


## Terminal history inspection

`H` opens history for the selected pending decision or operator inbox message; `g` looks up a decision or message by numeric ID, including completed records. The interface displays JSON records in the existing decisions, messages and outcomes sections, with record and text navigation plus explicit previous/next response pages. `R` reloads the current page; each page remains a separate snapshot and offsets can shift during concurrent changes. The interface holds the current history response page rather than aggregating every history page. It is not a global chronological timeline and does not authenticate participant labels.


## Explicit attention classification

A sender explicitly classifies work by operation: `submit` creates a decision requiring operator action, `submit_notice` reports a completion for the separate notice view, and `send_message` routes a message to its addressed inboxes without putting it in the decision queue. Accepted pending decisions from all agents share the active queue, ordered by importance and ID; deferred or answered decisions are excluded. The service does not infer whether content is genuinely urgent or useful, and sender labels are unauthenticated. This policy preserves operator control but still requires representative validation of attention quality and appropriate agent behavior.


## Operator inbox page navigation

The terminal requests and retains one operator inbox response page at a time. At the first or last record, h/l loads the adjacent page; previous page offsets are retained for backward navigation. Selection is retained by ID on refresh within the current page. Until the final page is reached, the total is shown as unknown. Inbox messages are retained and ID-ordered in V1, but page reads are separate snapshots; changes to retention semantics would need a new traversal design. This limits retained message payloads to a page, not total process memory: offset history grows with navigation and SQLite/libzmq have their own buffers.


## Terminal completion-notice paging

The terminal loads one page of open completion notices at a time, using h/l across page boundaries and retaining prior offsets for backward navigation. The main view shows a lower bound with `+` when further notices exist; it does not claim an exact count until all notices fit on the first page. Dismissal reloads from the first page and does not change the selected decision. If earlier notices disappear and a later offset becomes empty, the interface falls back to a prior page. Separate snapshots and concurrent dismissals can still shift offsets or selection. This limits retained notice payloads, not total memory; offset history grows with navigation.

The notice's agent label is part of its paged text alongside work, result and activity. It is not printed unbounded in the heading, so long labels can be read by paging without pushing the heading and controls off screen in the tested pane. Layout for other sizes and Unicode remains to be evaluated.

## Terminal decision queue paging

The terminal retains one decision response page and loads adjacent pages with h/l. It shows an offset-based position and a plus sign when more records follow. Page actions are unavailable while the next page loads. After a successful decision, deferral or priority change, the view returns to the first page. An empty offset after concurrent removals falls back through prior offsets. Refresh preserves the selected request by ID when it remains on the page; a custom draft stays attached to its request if the page shifts, with a warning that it is no longer on that page. Separate snapshots can shift offsets, duplicate or skip entries during traversal, or move a selection; priority reordering and concurrent changes do not provide snapshot traversal. A response page bounds retained decision payloads, not total process memory: offset history, SQLite, libzmq and individual records have their own resource costs.

The terminal requests optional `newest_id` metadata with each decision page: the highest submitted decision ID, including answered and deferred records, in the same read snapshot. After the initial observation, an increase sets a local `NEW (R)` indicator even if the new decision is on another page or has since been answered. It does not change the selection or draft; `R` in the decision queue clears it. The indicator is not durable or a per-request unread count, does not identify the new decision, and can be delayed while polling is paused or fails. It assumes the same deployment and its retained IDs while the interface is open. Calls omitting `include_newest` retain the previous response shape; an older service that ignores the option will not supply the indicator.

## Version-2 response envelope

Version 2 retains the 1 MiB request frame limit and permits a 3 MiB response frame. The service also accepts version 1 and limits its responses to 1 MiB. The CLI and terminal use version 2, so upgrade the service before those clients; a version-2 client cannot use an older version-1-only service. This permits a history decision to include both a near-limit request and a near-limit answer, and leaves room for response metadata. Page collection still targets 900 KiB except for a larger individual record. Legacy records imported from outside these request limits can still exceed a response frame. Frame limits and high-water marks do not bound total process memory.

When an oversized record is encountered as lookahead after valid page entries, return those entries and their next offset first. Fetching the oversized record itself still returns an error; the earlier records remain accessible, but the offset API cannot skip the offending record automatically.

## CLI standard-input operations

`aviary call ADDRESS -` reads one UTF-8 JSON operation from standard input until EOF, bounded to 1 MiB of raw input including whitespace. The serialized version-2 request envelope must still fit the existing 1 MiB wire limit. Malformed or oversized stdin fails before any operation is sent. Argument-based calls remain supported. Input is not automatically saved; callers must retain stable identity and content for retries.

## Platform and terminal direction (2026-11-10)

Target mainstream Linux distributions, macOS and BSD, including glibc and musl Linux environments. Start verification on Fedora, then expand the matrix with actual installations and tests; this direction is not a claim that those targets are supported today. The terminal should work directly and inside tmux and Herdr panes, with color by default and usable non-color cues. Ratatui with Crossterm is the chosen rendering stack; Clap supplies CLI argument parsing. Exact supported versions, terminal dimensions, accessibility and the platform test matrix remain to be established. Two-machine and broader crash/transport investigations are deferred, and representative operator validation is delayed until the interface has been developed further.

The terminal enters and clears an alternate screen before its first Ratatui draw, then restores the previous screen and cursor on ordinary exit or a handled termination signal. Abrupt termination may still prevent cleanup.

## Default operator instruction (2026-11-10)

The normal composer will require only instruction text. It will generate a new deployment-scoped submission identity for each new message, use channel `all`, and address the group (no individual recipient); the operator confirms before dispatch. A First Mate may monitor this channel through its own participant inbox. Group addressing delivers to all other members captured at acceptance, not solely to a First Mate. The channel is not authentication or authorization. Its provisioning, membership and behavior when unavailable or incompatible with existing immutable membership require design; do not silently replace an existing channel or add members. A failed or unknown send must retain the same identity and exact message for retry while the draft is open; recovery after exit remains open. Later per-message identity, channel and recipient overrides are tracked as US-057/L-16. The terminal now uses this default; direct and group routing remain available through the CLI, and no existing channel is renamed or migrated.

## On-demand keybinding help and local service startup (2026-11-10)

Normal views should not carry the complete keybinding list. Show complete keyboard help on demand, with concise contextual hints in the interface where useful. The help trigger and presentation are design choices; `?` or a key combination are candidates, not selected bindings. Help must be keyboard accessible, and opening and closing it must preserve the active view and any draft. The terminal now provides this on-demand help.

The intended local operator workflow starts its service automatically if needed and reuses the intended running service; it continues independently after the interface exits. Exact process model, instance and database identification, concurrent startup, endpoint selection, stop/status controls, logging, compatibility checks and failure reporting remain open. Do not silently connect to an unrelated process, overwrite a database, or change the loopback default; an explicitly remote endpoint must not cause a local service to be launched as its replacement. The current workflow still requires the service to be started separately.

## Terminal color direction (2026-11-10)

Use color by default on capable terminals and derive the interface's default colors from a standardized system that respects the user's terminal scheme, rather than imposing a fixed custom palette. Basic ANSI terminal colors are a candidate because terminal themes commonly control them; the exact standard and capability detection remain to be selected and verified. Do not require 256-color or truecolor support. Provide legible text, selection and status cues in basic-color and monochrome supported environments; color can reinforce those cues. The minimum required terminal controls, behavior for unsupported or misreported capabilities, and contrast across schemes remain open. Current color rendering has not yet been verified against this target.

## Operator overview, navigation and action clarity (2026-11-10)

The V1 targets are a concise overview of decisions, messages and completions; search by text, agent and work context; a selected-item preview; visible view and selection context; and clear pending, accepted, explicitly rejected and unconfirmed action states. Incomplete data must not be presented as an exact count, and an unknown result is not acceptance. Exact layout, search scope and semantics, how data from separate snapshots is aggregated, and how explicit rejection differs from an unknown transport or decoding failure remain open. These targets do not imply automatic retries, agent orchestration or authorization.

Long content should wrap and scroll by visible lines, retaining a useful reading position across resize. Temporary navigation and polling failures should retain the selected item and exact unsent draft; how to handle concurrent removal, closing the interface or restart needs design. Identify the target and effect of consequential actions before confirmation. An explicit confirmation remains required for sending instructions and answering decisions; action-specific interaction is to be designed without inventing extra confirmation steps for every navigation or deferral. The current implementation provides only partial evidence for these targets.

A durable seen/unread model and integration with a preferred external editor are V2 explorations. Their semantics, privacy, temporary files, editor failure and draft recovery require design; the existing local arrival indicator is not a durable unread count. Existing V2 filtering and overview/detail stories remain distinct from V1 search and selected-item preview.

## Keyboard help increment (2026-11-10)

`?` opens keyboard help outside text entry; Ctrl-G also opens it while composing or editing, so a literal question mark remains available in drafts. Esc closes help without changing the underlying view or draft. Normal queue, inbox, notice and history views use shorter hints, and inbox and notice headings identify the selected record. Small-pane help scrolling and broader workflow validation remain open.

The instruction composer generates 128 random bits from `/dev/urandom` for a new `tui-` identity and fails visibly if randomness cannot be read. It sends only after review and confirmation. The existing transactional message operation validates membership and rejects missing channels, senders outside the channel and empty recipient groups; it does not create or amend channels. A draft retains its identity and body on a failed send. Only known domain errors are labelled rejected; other errors remain unconfirmed. Random identity collisions are improbable, not impossible, and cross-platform availability of the random source remains to be checked.

## Terminal action outcomes and targets (2026-11-10)

The terminal checks the returned ID and operation-specific fields before reporting acceptance. Known domain errors are explicit rejections; transport, decoding, unexpected response and other server errors remain unconfirmed, because a write may have committed. The interface does not resubmit automatically. Decision, notice and inbox headings show the current record; queue hints identify the effect on that record. The service still arbitrates concurrent changes, and a stale answer is rejected rather than replacing the existing answer. Help pages pause queue refresh while open to preserve the selection; they resume after closing.

## Proposed queue and terminal direction (2026-11-10; pending review)

Stakeholder feedback calls for vertical item browsing and vertically organized content, visually distinct areas and color that respects terminal schemes, continuous UI navigation with data fetched in the background, transient human-readable success feedback, and an instruction composer without a separate review page. The send action remains explicit, and an uncertain outcome must not be represented as acceptance or automatically resubmitted. Failure and uncertainty need recoverable presentation even after a success notification would have disappeared. Exact layout, supported dimensions, focus transitions, colors and notification timing require design and validation.

Agents should be able to list the decision queue in bounded pages and swap items at the same effective importance. Candidate implementation must identify both items and their expected current positions or an equivalent version, atomically validate the preconditions, and reject a stale swap. Clarify how agent ordering interacts with operator ordering; the operator retains the existing ultimate control over importance. Priority changes, answers, insertion, deletion and concurrent swaps must be considered. Do not imply offset pages form a consistent snapshot or change submission retry payloads when persisting order.

The intended `all` destination is a shared work queue: an instruction can be accepted with no agent listening, remains active until resolved and then disappears from the active queue. **Open:** one claimant or multiple participants; take-up and resolution authority; multiple agents racing; agent crash or disconnect; timeout, release and redelivery; completion versus failure; idempotent acceptance and resolution; active listing and privacy; and whether a resolved instruction's content, result and retry identity persist outside the active queue. Removing an item from the active queue does not by itself authorize deleting existing messages, history or retry keys. The existing `all` group channel is not this queue; define coexistence, compatibility and a nondestructive migration before changing it. Direct and group messages continue to have their existing distinct semantics. Participant labels remain unauthenticated and the default listener remains loopback.

## Proposed queue ordering, deferral and asynchronous update direction (2026-11-10; review required)

The proposed queue keeps pending decisions ordered first by effective importance and then FIFO age within each importance. An agent may deliberately swap two positions at the same importance; this manual ordering is an override of the default age order and must not alter importance or the immutable submission payload. Define how swaps persist, how new arrivals fit around overrides, stale-swap rejection, and whether a deferral resets the item's age.

Deferral remains available and does not answer or resolve a request. The proposed behavior keeps it pending, moves it to the bottom of its same-importance queue, and sends a durable notification to the requesting agent. The agent must be able to retrieve that event after disconnect/restart. Define repeated deferral, event identity and retention, notification delivery, answer races, and restoration. The current implementation still stores deferred requests in a separate durable queue and does not notify the requester; this proposal is not implemented.

The operator requests asynchronous queue and inbox updates without periodic polling, including for agents observing queue changes. Candidate behavior is notification-driven while connected, with bounded background reads still allowed for payload retrieval, pagination and reconnect reconciliation. Notifications are hints; durable state remains authoritative and must recover missed changes. Evaluate the current ZeroMQ design and alternatives for subscriptions, bounded queues, coalescing, reconnect, event cursors, and missed-event reconciliation. Do not remove polling or claim transport suitability until an evaluation and verification plan are complete. This direction is proposed, not implemented.


## Reviewed P-04/P-05 interaction direction (2026-11-10)

The operator reviewed and accepted the [terminal mockup](mockups/p04-p05-terminal.md) as sufficient to proceed. It proposes focused list and detail regions, a stacked 40 × 12 target, direct Ctrl-S instruction sending, and four-second success feedback. Review establishes design direction, not implementation or validation. Preserve the existing `all` group semantics and the unresolved P-03 and deferral decisions. Actual supported dimensions, color behavior, loading, safe retry and operator usability require verification.


The first P-05 increment sends instructions directly with Ctrl-S from a single-line editor showing the existing `all` group target. Enter does not submit. Left/Right and Home/End inspect the draft; a failed or unconfirmed result locks its content and identity for an explicit identical retry, and discarding it requires confirmation. Drafts remain in memory only. Accepted actions show a human-readable status for four seconds. The remaining reviewed layout, suspended-draft navigation, accessibility and full P-04/P-05 verification are not complete.

An instruction draft can be suspended with Ctrl-O and resumed with m on the
decision screen. Its content, identity and failure state remain in memory, and
a reply composer cannot replace it. The interface displays the identity when
confirming discard after an unsuccessful send. Exiting still loses the draft;
suspension is not durable recovery.

The first P-04 layout increment renders decision, inbox and notice pages in
separate framed list and detail regions, side by side or stacked in a 40 × 12
pane. Tab switches focus; list Up/Down moves records and detail Up/Down selects
answers or scrolls message and notice text. Page Up/Down scrolls details; D
switches active/deferred decision queues. Existing h/l record navigation is
retained. Background page loads show loading rather than an actionable stale
record, and a continuation marker does not claim a complete snapshot. ANSI
basic cyan follows the terminal theme; NO_COLOR or TERM=dumb suppresses that
accent, with focus labels and selection markers retained. This is an increment:
exact reading-position preservation on resize and broader terminal/color
validation remain open, as do the rest of P-04/P-05 acceptance.


Queue detail scrolling records a character-position anchor and maps it into the
new wrapping on resize, retaining the anchor even when a wider pane shows the
whole item. The mapping can reveal earlier text; complex grapheme and terminal
width behavior remains to be validated. The current Crossterm backend encodes
the cyan accent using an indexed-color SGR sequence (`38;5;6`), so basic ANSI
color compatibility is not established. NO_COLOR suppresses the accent and
retains the non-color cues.

The output adapter now converts the interface's two Crossterm indexed accents
to eight-color ANSI cyan and yellow SGR sequences, buffering through each flush
to handle split writes. It leaves other output untouched. This avoids requiring
indexed-color support for these accents; terminal scheme contrast and wider
terminal compatibility remain to be verified. Ordinary quit with a suspended
unconfirmed instruction reopens its discard confirmation, including identity.
Ctrl-C, termination signals and abnormal exits still discard in-memory drafts.


Queue detail wrapping uses Unicode grapheme clusters and terminal-cell widths,
while resize anchors remain scalar offsets into the underlying text. The
instruction discard confirmation is a dedicated compact screen so that an
uncertain identity and the keep/discard choice are visible at 40 × 12.
Terminal-specific grapheme widths and behavior outside the exercised cases
remain subject to validation.

When an instruction draft is suspended, an accepted or failed action's status
remains visible alongside a short draft-held cue. Once a transient success
expires, the longer resume cue returns. This prevents the draft indicator from
masking an action outcome; the draft and identity remain unchanged.

## Terminal interaction follow-up (2026-11-10)

The operator requested Enter to send instructions and inline custom-answer entry. The terminal now uses Enter for both explicit actions, preserves the unchanged identity and content for a user-initiated retry, and retains the decision view while editing. Help scrolls instead of using pages; focused pane titles are bracketed, selected rows use the terminal accent with a non-color marker, and decision identifiers are omitted from the main list and detail title. Context and answer detail labels are separated by newlines. Older mockups and entries above describe prior designs. Mouse navigation, continuous history, and the new deferral and event-driven transport semantics remain open implementation work.

## Rust ZeroMQ transport and retained SQLite (2026-11-10)

The user selected zmq.rs and chose to retain SQLite after an isolated Turso 0.8.3 probe found that its dependency graph still builds C code. The service and clients use a local zeromq 0.7.0 source copy with receive limits, multipart draining and bounded per-peer send queues; see `vendor/zeromq/AVIARY-PATCHES.md`. The ROUTER/DEALER wire envelope and durable-acknowledgment boundary remain unchanged. A bounded reply enqueue can drop a response after commit; clients must treat it as unknown. The service now waits asynchronously for transport input and shutdown, while terminal refresh and CLI waiters still poll. SQLx continues to bundle SQLite; the native libzmq test dependency is retained for independent integration checks. A compiler-free build is therefore not claimed.

## Continuous history and mouse (2026-11-10)

History displays records together and scrolls by rendered lines; it loads the next or previous bounded response when scrolling beyond the current page. Pages are separate snapshots, can shift with concurrent changes, and the offset trail can grow; this is not a global chronological timeline or a total process-memory bound. The terminal supports mouse focus and row selection and clickable inbox and notice labels; clicks do not confirm actions. Drafts and overlays retain keyboard control.

## Unified decision queue and durable deferral events (2026-11-10)

The new `decision_queue` operation and terminal show all unanswered decisions in one queue, sorted by effective importance and an independent persistent order. Initial order is FIFO; `defer_decision` moves a decision to the tail of its importance group and atomically records a retained event for its requester's routing label. The legacy deferred flag is preserved and legacy operations remain available for existing clients; they do not provide the new notification semantics. Existing records are indexed on first startup without rewriting request payloads or retry keys. Identical defer or swap action retries reuse a deployment-scoped identity. A same-importance swap checks both listed versions and rejects stale or answered requests. Priority edits and legacy deferred-flag changes advance the version. The requesting `wait` command returns a deferral event ID and can resume with `--after-event`; answers take precedence. Labels do not authenticate.

The `NEW (R)` indicator has been replaced by an automatically expiring arrival notification, which preserves the current draft. The former separate deferred-queue keybinding is removed. Queue pages remain separate snapshots, and arrival metadata is read separately from the page. Notification delivery still uses polling; event-driven updates are separate work.
