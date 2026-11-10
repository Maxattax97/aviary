# Agent attention and messaging system — user stories

**Status:** Working baseline v0.2 (2026-11-09); amended after stakeholder review

**Working name:** Undecided (candidates: Aviary, Coop, Flockbox, Chirrup)

**Purpose:** Input for systems requirements engineering, verification planning, architecture, and implementation backlog.

## Product intent

> Agents manage execution. The system manages attention. The human manages decisions.

The human must not have to monitor a First Mate agent's scrolling output or individual agent conversations. The system presents concise, prioritized decisions and relevant completion notices; it also accepts outgoing instructions. It supports independent agents and, in V1, agents running on separate machines.

## Status legend

- **V1:** Target for initial release, subject to an executable vertical-slice and sizing review.
- **V2:** Desired near-term follow-up.
- **Future:** Preserve as a design consideration; not a V1 commitment.

**Counts:** 70 stories: 47 V1, 14 V2, 9 Future.

## User stories

### A. Attention management

- **US-001 [V1]** As a human operator, I want a single consolidated view of requests from all my agents so that I do not have to monitor individual sessions.
- **US-002 [V1]** As a human operator, I want to see only actionable information by default so that routine agent execution does not consume my attention.
- **US-003 [V1]** As a human operator, I want requests ordered by importance so that I can work on the most consequential matters first.
- **US-004 [V1]** As a human operator, I want to see a concise explanation of each request so that I can make a decision without reading an agent's entire conversation.
- **US-005 [V1]** As a human operator, I want to inspect supporting context on demand so that I can investigate decisions that require more information.
- **US-006 [V1]** As a human operator, I want to defer a request without losing it so that I can focus on a more important activity.
- **US-062 [V1]** As a human operator, I want a concise overview of pending decisions, messages and completions so that I can understand where attention may be needed without visiting every view.
- **US-063 [V1]** As a human operator, I want to find communications by recognizable text, agent or work context so that I can return to an item without knowing its numeric ID.
- **US-070 [V2]** As a human operator, I want to distinguish new or changed items from those I have already seen so that I can find what needs another look after returning.
- **US-007 [V2]** As a human operator, I want to identify my primary objective or "big rock" so that requests supporting it receive greater attention.
- **US-008 [V2]** As a human operator, I want low-priority requests to resurface periodically so that neglected but worthwhile work is not forgotten.
- **US-009 [V2]** As a human operator, I want related or duplicate requests consolidated so that multiple agents do not demand my attention for the same issue.
- **US-010 [V2]** As a human operator, I want to review lower-priority work when higher-priority work is exhausted so that otherwise idle time can be used productively.

### B. Decisions and approvals

- **US-011 [V1]** As an agent, I want to present a question with any reasonable number of suggested answers so that the human can make an informed choice.
- **US-012 [V1]** As an agent, I want to identify my recommended answer and explain why so that the human can decide quickly.
- **US-013 [V1]** As a human operator, I want the recommended answer presented first and selected by default so that accepting it requires minimal interaction.
- **US-014 [V1]** As a human operator, I want to choose a custom answer when none of the suggested answers is appropriate.
- **US-015 [V1]** As a human operator, I want simple approval and denial requests to require minimal interaction.
- **US-016 [V1]** As an agent, I want to receive the selected answer in a structured form so that I can reliably continue execution.
- **US-017 [V2]** As a human operator, I want to revise a pending decision request or ask for clarification without losing its context.
- **US-018 [V2]** As a human operator, I want to correct an accidental response when doing so can still influence the recipient's actions, so that mistakes do not unnecessarily propagate.

### C. Communication and delegation

- **US-019 [V1]** As a human operator, I want to send instructions to my agents from a central interface so that I can delegate work without navigating to individual agent sessions.
- **US-020 [V1]** As an agent, I want to communicate with a particular agent or logical group of agents so that messages reach their intended audience without unnecessarily involving others.
- **US-021 [V1]** As an agent, I want to receive messages relevant to me or the groups I participate in so that I can respond to work directed toward me.
- **US-022 [V1]** As an agent, I want to request a response and wait for it so that I can resume execution when an appropriate recipient answers.
- **US-023 [V1]** As an agent, I want to retrieve an outstanding response later so that I can continue after an interruption or restart.
- **US-024 [V1]** As a human operator, I want to receive completion notifications so that I know when an agent has finished meaningful work.
- **US-025 [V1]** As a human operator, I want completion notifications to identify the relevant work and its results so that I can inspect the outcome if desired.
- **US-056 [V1]** As a human operator, I want to send an instruction by entering only its text, with its identity and the shared destination selected automatically, so that I can quickly direct work to the team or a First Mate.
- **US-057 [V2]** As a human operator, I want to adjust the generated identity, channel or recipient when composing an instruction so that I can handle exceptional routing needs.
- **US-026 [V2]** As an agent, I want to exchange requests and responses directly with other agents so that coordination does not always require a human intermediary.

### D. Terminal ergonomics

- **US-028 [V1]** As a human operator, I want to navigate the queue using only my keyboard so that decision-making is fast.
- **US-029 [V1]** As a human operator, I want consistent keyboard shortcuts for selection, confirmation, deferral, and navigation so that repeated actions become effortless.
- **US-030 [V1]** As a human operator, I want the interface to update as new requests arrive so that I do not need to manually refresh it.
- **US-031 [V1]** As a human operator, I want the interface to work inside an ordinary terminal or tmux pane so that I can integrate it with my existing workflow.
- **US-058 [V1]** As a human operator, I want keyboard shortcuts available on demand, with at most concise contextual hints in the normal interface, so that I can discover controls without a permanent wall of keybindings crowding my work.
- **US-060 [V1]** As a human operator, I want Aviary to use colors from a standardized color system that respects my terminal's scheme, so that the interface fits my environment instead of imposing its own palette.
- **US-061 [V1]** As a human operator, I want the interface to remain usable on terminals without 256-color support, so that limited terminal environments do not prevent me from operating Aviary.
- **US-064 [V1]** As a human operator, I want a useful preview of the selected item while browsing a list so that I can decide whether to inspect its full details.
- **US-065 [V1]** As a human operator, I want to identify my current view, selection and relevant next action so that I know what an action will affect.
- **US-067 [V1]** As a human operator, I want long content to wrap and scroll by visible lines so that I can read continuously and keep my place when the pane is resized.
- **US-068 [V1]** As a human operator, I want my selection, reading position and unsent drafts preserved when navigating or temporarily losing connection so that I can resume my work without reconstructing it.
- **US-069 [V1]** As a human operator, I want consequential actions to make their target and effect clear before confirmation so that I can avoid acting on the wrong item.
- **US-071 [V2]** As a human operator, I want to use my preferred editor for substantial text so that composing long instructions is not constrained by a small input field.
- **US-032 [V2]** As a human operator, I want configurable keybindings and filtering so that I can optimize the interface around my preferences.
- **US-033 [V2]** As a human operator, I want to move quickly between an overview and a detailed request so that I can preserve context while processing a queue.

### E. Reliability, independence, and integration

- **US-034 [V1]** As an agent developer, I want to interact with the system using a documented CLI so that I can integrate any agent framework or programming language.
- **US-035 [V1]** As an agent, I want a portable skill explaining the protocol and command conventions so that I can use the system without loading permanent MCP tools.
- **US-036 [V1]** As a human operator, I want messages and responses to survive crashes and restarts so that no important decision disappears.
- **US-037 [V1]** As an agent, I want failed or retried submissions to avoid producing duplicate requests so that transient failures do not flood the queue.
- **US-038 [V1]** As a human operator, I want to review the history of a request, including related messages, decisions, and outcomes, so that I can understand how the work progressed.
- **US-039 [V1]** As a human operator, I want the daemon to operate independently of the TUI so that closing my terminal does not interrupt agents.
- **US-040 [V1]** As a human operator, I want to start the system with minimal setup so that using it does not require manually orchestrating multiple background processes.
- **US-059 [V1]** As a human operator, I want Aviary to start its local communication service when needed and keep it running after I close the interface, so that I can use Aviary without manually starting or supervising a separate daemon.
- **US-066 [V1]** As a human operator, I want to distinguish pending, accepted, rejected and unconfirmed actions so that I know when to continue or investigate before retrying.
- **US-041 [Future]** As a human operator, I want to identify sources of excessive or unhelpful messages so that I can address agents that are consuming unnecessary attention.

### F. Future distributed agent collaboration

- **US-042 [V1]** As a human operator, I want agents operating on different machines to participate in the same communication system so that their physical location does not fragment my workflow.
- **US-043 [Future]** As an agent, I want to discover which participants or groups can handle a particular request so that I can delegate work without knowing their runtime or location.
- **US-044 [Future]** As an agent, I want to exchange structured requests and replies with other agents so that multi-agent coordination does not depend on a central conversational orchestrator.
- **US-045 [Future]** As an operator, I want to configure permissions for agents and groups so that participants cannot impersonate others or access unrelated work.
- **US-046 [Future]** As an operator, I want visibility into unresolved inter-agent requests and communication failures so that autonomous work does not silently stall.
- **US-047 [Future]** As an agent, I want messages intended for temporarily unavailable recipients to remain deliverable so that intermittent connectivity does not prevent coordination.
- **US-048 [Future]** As an operator, I want to connect alternative interfaces, such as web and graphical clients, so that the same communications are accessible outside a terminal.

### G. Context, history, and workspace navigation

- **US-049 [V1]** As a human operator, I want to identify the agent and work context associated with a message so that I can understand its origin.
- **US-050 [V2]** As a human operator, I want to navigate directly from a message to its originating work environment, when available, so that I can investigate without manually locating the agent.
- **US-051 [V1]** As an agent, I want to associate messages and requests with related activity so that subsequent exchanges retain their context and can be traced back to the work that generated them.
- **US-052 [V2]** As a human operator, I want to review a chronological history of related activity across multiple agents and sessions so that I can understand the progression of a larger effort.
- **US-053 [V1]** As an agent operating across a network, I want to recover relevant pending communications after losing connectivity so that temporary disconnections do not cause me to miss work.
- **US-054 [Future]** As a human operator, I want communication between machines to be restricted to authorized participants so that unrelated or untrusted systems cannot access my work.

### H. Multiple users

- **US-055 [Future]** As a human operator sharing a deployment with other users, I want my messages and work separated by user so that each user can use the system without seeing another user’s communications.

## Decisions and boundaries carried forward

- The product is a standalone, model- and orchestration-agnostic Rust system; tmux/Herder integration is optional.
- A central service, machine-readable CLI, and independent terminal UI are the selected initial architecture; exact process lifecycle remains to be designed.
- V1 includes agents on multiple machines. The initial architecture uses a central service; transport, security details, and storage remain subject to engineering review.
- Agent integration should work through documented commands and a portable, on-demand agent skill, without mandatory MCP.
- Users should be able to use any number of suggested responses, including simple yes/no decisions. The recommended response is shown first and preselected; custom response appears last. Numeric selection is the proposed keyboard convention, subject to interaction review.
- The operator ultimately controls request importance; agents may suggest or infer defaults.
- The UI favors keyboard-first, low-friction navigation; Vim-like bindings are a suggested default rather than a hard architectural constraint.
- V1 is a single-user service that listens on localhost by default, with an explicit option to listen on a network interface for a trusted local cluster. V1 has no application authentication. Future network authorization and multi-user deployments require security and isolation, without assuming a traditional username/password system.
- Communication uses topics or channels to support one-to-one, one-to-many, many-to-many, and many-to-one exchange. Individual and group targeting remain required.
- The normal instruction destination is the `all` channel, addressed to its group; a First Mate may monitor it, but this is a deployment choice, not a mandatory hierarchy. The operator enters instruction text while the system supplies the identity and routing defaults. Per-message adjustments are V2.
- Human-facing interaction should not evolve into a general-purpose ongoing chat client. Message correlation and historical tracing are still required.
- Project, activity, and request relationships should be represented without assuming a strict one-to-many tree or building a project-management system.
- AI-driven scheduling, repeated resurfacing, and elaborate attention policies are V2 explorations rather than core protocol requirements.

## Requirements-engineering handoff instructions

1. Preserve existing story IDs, priorities and story intent; do not reuse removed IDs.
2. Develop a concise Concept of Operations (ConOps) with normal and failure scenarios before freezing requirements.
3. Derive atomic, unambiguous, solution-neutral, verifiable system requirements with stable IDs (e.g., SYS-001). A story may map to several requirements; a requirement may trace to several stories.
4. For each requirement, capture source story ID(s), rationale, priority, verification method, acceptance criteria, and risks/open questions. Keep cross-cutting qualities like persistence, security, latency, and multi-machine delivery visible.
5. Define verification approach and acceptance tests alongside the requirements, not only after implementation tickets are written. Distinguish **verification** (did we meet specified requirements?) from **validation** (does the resulting workflow solve the operator's problem?).
6. Review potential overlaps without silently deleting stories: US-022/026/044 (human versus inter-agent request/reply); US-023/047/053 (restart, offline delivery, reconnect); US-038/051/052 (request history, causal relationships, work-level timelines); US-041 (observability versus suppression).
7. Record technical decisions separately from stakeholder requirements: transport, daemon lifecycle, database, identity, authorization, transport security, and tmux/Herder navigation. A central service is the selected initial topology.
8. Define a first vertical slice: agent submits a decision -> durable server state -> TUI displays it -> human selects the recommended option -> waiting agent gets one unambiguous response, including recovery after a restart.
9. After reviewed requirements and high-level architecture are available, decompose work into small implementation tickets. Connect tickets to SYS IDs and tests; use CI to evaluate verification coverage.
10. Validate the working system with actual operator workflows, especially attention reduction, one-keystroke acceptance, context switching, and resumable agent work.

## Suggested minimal traceability fields

`Requirement ID | Requirement | Parent US IDs | Rationale | Priority | Verification method | Test IDs | Implementation ticket IDs | Status`

## Primary methodological references

- NASA Systems Engineering Handbook, stakeholder expectations: https://www.nasa.gov/reference/4-1-stakeholder-expectations-definition/
- NASA Systems Engineering Handbook, technical requirements: https://www.nasa.gov/reference/4-2-technical-requirements-definition/
- NASA Systems Engineering Handbook, requirements management and traceability: https://www.nasa.gov/reference/6-2-requirements-management/
- ISO/IEC/IEEE 29148:2018 (active standard as of 2026-11-09): https://committee.iso.org/standard/72089.html
