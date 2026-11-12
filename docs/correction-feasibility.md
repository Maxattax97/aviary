# Linked correction feasibility (L-03)

**Assessment, not an approved design or implemented feature.** US-018 concerns correcting a response when that can still affect the recipient. Preserve the original response and do not promise cancellation, rollback, or delivery before action. The scope below is an assessment of decision answers; whether corrections should also cover agent-message replies is a product decision.

## Existing capabilities and gaps

- A decision answer is stored once on its request. The conditional write rejects subsequent answers, and the decision waiter returns the answer and exits. There is no correction record or ongoing correction channel for that waiter.
- The request carries the submitting agent label and optional activity. Neither authenticates a recipient or proves that an agent is still listening or has not acted. Activity-based history associates records with the same activity, but activity is optional and is not an unambiguous correction link.
- Messages have stable submission identities, transactional recipient snapshots and retained inboxes. A message requires an existing channel and member labels; a decision has no required channel. A decision correction therefore cannot safely be routed as an ordinary message merely by assuming a channel or inferring its members.
- A message reply is limited to one response for a direct request. Reusing that operation for a correction would conflict with the existing one-response contract. Generic change notifications are hints, not a durable delivery receipt; inbox reads have no persisted read or receipt state.

## Feasible direction and cost

An additive durable correction record keyed to the original decision and a new retry identity is technically plausible. A transaction could validate the referenced answered decision, preserve its original answer, persist the correction and recipient routing label, and acknowledge only after commit. An identical retry would return the same correction identity; a conflicting retry would be rejected. Explicit linkage permits history and the terminal to show both original and correction without relying on activity matching. Existing records, legacy `get`/`wait` answer shapes and the single-answer rule would remain intact.

Making a correction useful requires a separately specified recipient contract: bounded discovery of retained corrections, change-hint wakeup, reconnect reconciliation, and a durable, idempotent recipient receipt if delivery is to be reported. The receipt can establish only the agreed client action, such as acknowledging retrieval; it cannot prove human or agent comprehension, that the correction arrived before action, or that the original action was reversed. A replacement or disconnected agent needs a defined way to discover corrections; relying on a waiter that has already exited is insufficient. New schema, operations and migration, CLI and skill guidance, terminal creation and status views, and compatibility and failure tests would all be needed. This is a qualitative scope assessment, not an implementation estimate.

The current service trusts any connected client and routing labels can be impersonated. A receipt under that model is a claim from a client, not authenticated proof of a particular agent. Existing retention and future shared-work decisions may affect discovery and cleanup; no correction may silently weaken retry guarantees.

## Decisions and verification before implementation

Review which responses may be corrected, who may issue corrections, recipient identification and receipt meaning, multiple corrections and their ordering, and how a recipient handles an already-performed or irreversible action. Define the user-visible statuses for accepted, unknown, unacknowledged and acknowledged corrections and behavior when a recipient never acknowledges. Decide how an old client discovers a correction without silently changing its existing answer contract. Translate those choices into requirements and acceptance tests before implementation.

At minimum, test preserved original answer and explicit history linkage; restart and identical/conflicting retries; offline recipient recovery; duplicate and lost receipt retries; races among correction, retrieval and agent action; multiple corrections if allowed; old-client compatibility; bounded pages and unavailable reads; and terminal target, confirmation and unknown outcome. Test remote operation and OS-crash durability separately from local process restarts. No test of service persistence alone proves that a correction influenced an external action.
