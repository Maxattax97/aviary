> **Update 2026-11-09:** The user approved the initial baseline. First-slice choices and tickets are recorded in `docs/decisions.md` and `docs/tickets.md`; a partial Rust prototype and local process-kill integration test now exist. The two-machine and interactive terminal milestone remains unverified. The original handoff below is historical and its pre-approval status and next steps have been superseded.

# Handoff

## Objective and status

Develop a model- and orchestration-agnostic Rust agent messaging and human attention system from the stakeholder baseline. The requirements and initial architecture are drafted, but remain **subject to human review**. Do not create implementation tickets or begin coding until the user reviews the requirements baseline and initial architecture. No code or tickets have been created.

## Start here

- `docs/user-stories.md`: stakeholder baseline, 54 stories (34 V1, 11 V2, 9 Future). Preserve IDs and priorities; do not reuse the removed story ID.
- `docs/conops.md`, `docs/requirements.md`, `docs/traceability.md`, `docs/verification.md`: scenarios, 44 proposed requirements, bidirectional mappings and planned verification.
- `docs/decisions.md`: design choices, unresolved details and trust boundary. US-018 is V2 and remains feasibility-limited: no guaranteed cancellation or rollback.

V1 is single-user, with a central service and separate terminal attention interface. The service binds localhost by default; the operator can explicitly select a network interface for agents on a trusted local cluster. V1 has no application authentication or authorization: any process able to reach the listener is trusted, and participant and channel names are routing labels, not security boundaries. Network authentication, public internet deployment and user isolation are Future; do not assume passwords, a registration scheme or a trusted administrator. The system uses channels supporting one-to-one and multi-party communication. V1 includes targeted agent-to-agent requests and one correlated response. Operators control importance, with agent suggestions and manual overrides; configurable rules are not required. Keep project management and orchestration out of scope.

Use indefinite retention without automatic deletion as the proposed V1 default; keep retry identities as long as their associated records exist. Identical retries return the existing request; conflicting content under the same identity is rejected. Retention configuration is V2. Recommended answers are first and preselected but require explicit confirmation. Numeric choices and Vim-style navigation are proposed defaults.

## Next steps and verification

1. Ask for or obtain the user's review of the requirements and initial architecture; do not mistake agreement on individual decisions for approval of the complete baseline. Resolve review findings and any first-slice choices in `docs/decisions.md` with proportionate detail.
2. After review, create small implementation tickets tied to SYS and T IDs; then implement the smallest end-to-end slice in `docs/verification.md`, including actual two-machine testing and crash/retry recovery. Do not claim unexecuted tests or runtime behavior are verified.
3. At resumption, run `git status --short`, `git log -1 --oneline`, and read the documents above. Before finalizing changes, check story IDs/priorities and requirement-to-test mappings in both directions, and validate local Markdown links. The last structural check matched 54 story entries and 44 requirement/verification entries; this is not a system test.

## Repository state

At handoff creation the only worktree is `/home/max/src/aviary` on `main`, commit `6b2611a` (`feat: initial requirements`); it was clean before adding this handoff. No branch or worktree was created by the agent. No upstream is configured for `main`, so an ahead/behind or unpushed comparison is unavailable. The handoff itself is a new uncommitted file; do not discard it or other user changes.
