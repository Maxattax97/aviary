# Shared `all` work queue: proposed P-03 contract

This is a concrete proposed contract for US-056 and US-075–076, SYS-061–063 and T-061–063. It is not implemented or an approved change to the existing `all` group channel. The current composer still sends a group message to a provisioned channel. Do not reinterpret or remove existing messages, channels, recipient snapshots, history or retry keys.

## Distribution and lifecycle

An instruction is **one unit of shared work**, not a broadcast. It has a deployment-wide submission identity, immutable instruction text and a service-assigned ID. `all` names a separate shared-work destination, not the existing group channel namespace. The active list contains both available and claimed instructions until resolution. Retain resolved payloads, results, retry identities and claim history outside the active list; deletion and retention policy remain separate work.

A new submission is durable before acknowledgment and does not require any agent, participant registration or listener to be present. An identical identity and payload retry returns the original ID even after resolution; reuse with different content or a different operation kind is rejected. A lost reply means unknown outcome, never automatic resubmission.

At most one claim is current for an instruction. A participant supplies a nonempty routing label and a unique action identity when claiming. The service atomically arbitrates races: one fresh claim succeeds and the rest are rejected as already claimed or stale. A successful claim advances a persistent generation; the response includes an opaque claim token or generation that must accompany later actions. An identical retry of a successful claim returns the original receipt, which must distinguish the recorded receipt from whether that claim is still current. A stale retry cannot resurrect a released or resolved claim.

The claimant may explicitly release a claim without resolving the instruction. Release makes it available again and invalidates the old claim token. A separate explicit operator recovery operation can release a stranded claim after the operator checks the work; it also invalidates the token. There is no automatic expiry, heartbeat, disconnect-triggered release or automatic reassignment in this proposal. This avoids treating a temporary disconnect as proof that work stopped, at the cost that abandoned claims require attention. The active list must show claimant and claim state so stranded work is visible. An agent crash or service restart preserves the claim; an agent retaining its token can inspect and resume it. A replacement agent cannot safely assume that a released agent stopped executing.

Resolution requires the current claim token, an explicit result and a unique action identity. It atomically records the result and removes the item from the active list. An identical resolution retry returns the recorded receipt; a conflicting identity, stale token, duplicate resolution under a different identity or resolution of another generation is rejected without changing the item. Failure to finish is not automatically success or resolution: the claimant can retain or release the claim and report its failure separately. One active claim does not guarantee exactly-once external effects, cancel a former worker, authenticate labels, or prevent a participant from impersonating another.

All acceptance, claim, release and resolution records are transactional with their retry records. Reads and event hints never constitute an acknowledgment. A `watch` change is only a hint; on connect or reconnect clients reread durable state. New items, claims, releases and resolutions notify subscribers after commit. Existing service limits, loopback default and trusted-network boundary remain in force.

## Access and bounded reads

Agents discover active instructions through a bounded, stable-order, cursor-paged operation. Each entry includes an ID, a bounded text preview, available/claimed state and, if claimed, its participant label and generation. A separate bounded detail operation returns the full instruction and relevant current state; framing limits and oversized legacy data are handled explicitly. Pages are independent snapshots; a cursor must make forward progress, and changes while paging can affect what is observed. An agent must reconcile after reconnect and before acting on an old claim. Routing labels are neither access control nor authentication.

The normal operator composer retains its generated stable identity, exact draft, visible destination and explicit Enter send. The UI must identify the destination as **shared work queue `all`** before the send; an accepted response must be checked against the specific new operation. A rejected or unknown result retains the exact draft and identity for a user-initiated identical retry, with no automatic send. A service that lacks the operation must produce a clear rejection or unconfirmed result, not silently fall back to group fan-out. Direct and group message operations and the `all` channel continue to work as before when explicitly used.

## Compatibility and migration

Use additive schema and distinct protocol operations rather than changing the meaning of `send_message` or an existing channel. Upgrade the service first, then CLI, terminal and portable skill together. Existing group messages and the `all` membership retain their original recipient snapshots and history; they are not converted into shared instructions and cannot be inferred to be resolved. New shared work and old group communications can coexist even when both use the text `all`. No destructive migration or automatic channel creation is required.

Before changing the composer default, define how operators identify old versus new behavior in release notes and help, test an existing database with an `all` channel and retained messages, and verify that an older service cannot make a new client report a shared submission as accepted. Existing explicit direct/group clients must retain their behavior. Startup and version errors must not create a second independent queue or silently switch databases.

## Acceptance and failure cases

- Submit with no connected agent; acknowledge only after commit. Restart and retrieve the same active item. Lose the reply, retry the same identity, and verify one item; reject a changed retry.
- Have multiple agents race to claim the same item. Verify one current claim, persistent generation and a clear loser. Retrying the winning claim does not make a second claim.
- Restart with a claim outstanding; verify it remains claimed and visible. Release it explicitly, claim it again and verify every operation using the old token is rejected without affecting the new claim.
- Race release, recovery and resolve; verify one valid state transition, retained action identities and no partial result. Resolve one instruction while another remains active; repeat after restart. Retry an accepted resolution unchanged and reject a changed or conflicting retry.
- Verify bounded traversal, empty results, large text, unavailable and malformed responses, disconnect and reconnect reconciliation, and notification after commit. No notification is proof of acceptance; no latency or exactly-once side-effect claim is made.
- Use an existing database containing an `all` channel, group messages, direct messages and retry keys. Verify all remain readable and identical retries still work. Exercise old/new service and client combinations explicitly.
- Drive the real composer, CLI and portable skill: show the shared destination before Enter, do not submit before Enter, preserve unknown-outcome identity and text, and retrieve the resulting item from an agent. Document local process-kill versus OS-crash and remote-test limits.

## Choices needing operator review

The proposed single-claim policy and manual recovery are substantive workflow choices. Confirm whether work should instead be delivered to every agent, whether claims need timed expiry or heartbeat-based recovery, and whether failure should remain active or have a separate terminal outcome. Until those choices are settled, P-03 remains a proposal; current group fan-out must not be represented as shared work.
