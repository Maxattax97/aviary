# Retention and deletion design questions (L-08)

**Discovery only; no deletion policy is approved or implemented.** V1's recorded default retains pending and completed records indefinitely. Retry identities remain while associated records exist. Configurable retention and deletion are V2; the existing service must not silently start deleting records.

## What must remain consistent

The `submission_keys` table stores an operation kind, serialized payload and original record ID. Identical retry detection compares that payload and returns the stored ID; a different kind or payload is rejected. Deleting an application row alone can therefore return an ID that no longer resolves, while deleting its retry key can allow a later identical submission to create new work. A tombstone containing the existing full serialized payload still retains its content; replacing it with a digest would need a separately reviewed collision, canonicalization and privacy contract and would change the current equality mechanism.

Decisions also have queue positions, attention state and durable deferral events. Messages have recipient snapshots and request/response links; notices, decisions and messages may be grouped by optional activity in history. Pruning one part can produce broken references, incomplete history or an unreadable requester event. There is no durable read state for inbox messages, and generic change hints are not acknowledgments. A disconnected recipient's next read and a waiter's event cursor must have explicit behavior if data can expire while the client is away.

SQLite row IDs and event cursors must not silently refer to different records after deletion and reuse. Offset pages already represent independent snapshots; deletion can move records between pages. An implementation needs an explicit cursor and expired-record contract, including how clients detect a history or event gap and whether old IDs are ever reused. The service's existing database lock does not prevent noncooperating writers.

## Choices requiring review

- Which classes may expire, who configures and initiates expiry, and what protects pending decisions, undelivered work, active requests, outstanding events and records required for a linked history?
- Is the goal reducing active data, reclaiming storage, removing sensitive content, or all three? Define what happens to retry metadata, logs, WAL files, backups and exported data; logical deletion alone does not establish physical erasure.
- How long can an accepted submission be retried safely? After that horizon, should the service return a distinct expired outcome, reject the identity indefinitely, or permit reuse with an explicit loss of deduplication? Define what an identical retry returns if the original content is unavailable.
- When history members expire, should related records be retained as a unit, represented by explicit unavailable markers, or removed separately? Define recipient-visible gaps and the behavior of direct lookups and event cursors.
- Specify manual versus automatic cleanup, preview and confirmation, dry-run counts, batching, restart recovery, resource limits, clock assumptions and concurrent client behavior. Coordinate with the unresolved shared-work and correction lifecycles before applying a common policy.

## Candidate safeguards and acceptance

An additive design should preserve the existing no-expiry default, make any retention window and retry guarantee explicit to clients, and update related records and retry metadata atomically. It should prevent expired IDs from being mistaken for new records and distinguish an expired result from an unknown outcome. If background cleanup is adopted, an interrupted batch must leave a valid recoverable state. These are design constraints to review, not authorization to implement cleanup.

Before implementation, approve requirements and test cases for all affected kinds and links. Exercise identical and conflicting retries before and after the chosen horizon, requester and recipient disconnect/reconnect across expiry, cursor gaps and ID reuse, concurrent submission and cleanup, history traversal during pruning, process restart during cleanup, migration of existing databases and old-client behavior. Separately test any promised OS-crash or physical-erasure properties; a successful SQL `DELETE` or process-kill test does not prove either.
