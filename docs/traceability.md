# Requirements traceability matrix

**Status:** Draft; all planned tests are not run and implementation tickets are not created. Source: [stories v0.2](user-stories.md). Requirement details, rationale and acceptance criteria are in [requirements](requirements.md); test approach is in [verification](verification.md).

## Requirements to stories and verification

| Requirement | Originating stories | Priority | Planned verification | Ticket | Status |
|---|---|---|---|---|---|---|
| SYS-001 | US-001, US-002 | V1 | T-001 (test) | None | Draft / not run |
| SYS-002 | US-003 | V1 | T-002 (test) | None | Draft / not run |
| SYS-003 | US-004 | V1 | T-003 (inspection/test) | None | Draft / not run |
| SYS-004 | US-005 | V1 | T-004 (test) | None | Draft / not run |
| SYS-005 | US-006 | V1 | T-005 (test) | None | Draft / not run |
| SYS-006 | US-011, US-014 | V1 | T-006 (test) | None | Draft / not run |
| SYS-007 | US-012 | V1 | T-007 (test) | None | Draft / not run |
| SYS-008 | US-013 | V1 | T-008 (test) | None | Draft / not run |
| SYS-009 | US-014 | V1 | T-009 (test) | None | Draft / not run |
| SYS-010 | US-015, US-028, US-029 | V1 | T-010 (test) | None | Draft / not run |
| SYS-011 | US-016 | V1 | T-011 (test) | None | Draft / not run |
| SYS-012 | US-019, US-020 | V1 | T-012 (test) | None | Draft / not run |
| SYS-013 | US-020 | V1 | T-013 (test) | None | Draft / not run |
| SYS-014 | US-021 | V1 | T-014 (test) | None | Draft / not run |
| SYS-015 | US-022 | V1 | T-015 (test) | None | Draft / not run |
| SYS-016 | US-023 | V1 | T-016 (test) | None | Draft / not run |
| SYS-017 | US-024 | V1 | T-017 (test) | None | Draft / not run |
| SYS-018 | US-025 | V1 | T-018 (test) | None | Draft / not run |
| SYS-019 | US-028 | V1 | T-019 (test) | None | Draft / not run |
| SYS-020 | US-029 | V1 | T-020 (test) | None | Draft / not run |
| SYS-021 | US-030 | V1 | T-021 (test) | None | Draft / not run |
| SYS-022 | US-031 | V1 | T-022 (demonstration) | None | Draft / not run |
| SYS-023 | US-034 | V1 | T-023 (inspection/test) | None | Draft / not run |
| SYS-024 | US-035 | V1 | T-024 (inspection/demonstration) | None | Draft / not run |
| SYS-025 | US-036 | V1 | T-025 (test) | None | Draft / not run |
| SYS-026 | US-037 | V1 | T-026 (test) | None | Draft / not run |
| SYS-027 | US-038 | V1 | T-027 (test) | None | Draft / not run |
| SYS-028 | US-039 | V1 | T-028 (test) | None | Draft / not run |
| SYS-029 | US-040 | V1 | T-029 (demonstration) | None | Draft / not run |
| SYS-030 | US-042 | V1 | T-030 (test) | None | Draft / not run |
| SYS-031 | US-049 | V1 | T-031 (test) | None | Draft / not run |
| SYS-032 | US-051 | V1 | T-032 (test) | None | Draft / not run |
| SYS-033 | US-053 | V1 | T-033 (test) | None | Draft / not run |
| SYS-034 | US-054 | Future | T-034 (test) | None | Deferred / not run |
| SYS-035 | US-054, US-020, US-021 | Future | T-035 (test) | None | Deferred / not run |
| SYS-036 | US-016, US-036, US-037 | V1 | T-036 (test) | None | Draft / not run |
| SYS-037 | US-055 | Future | T-037 (test) | None | Deferred / not run |
| SYS-038 | US-003; stakeholder clarification | V1 | T-038 (test) | None | Draft / not run |
| SYS-039 | Stakeholder platform constraint | V1 | T-039 (demonstration) | None | Draft / not run |
| SYS-040 | US-020, US-021; stakeholder clarification | V1 | T-040 (test) | None | Draft / not run |
| SYS-041 | US-037; stakeholder clarification | V1 | T-041 (test) | None | Draft / not run |
| SYS-042 | US-037; stakeholder clarification | V1 | T-042 (test) | None | Draft / not run |
| SYS-043 | Stakeholder deployment constraint | V1 | T-043 (test) | None | Draft / not run |
| SYS-044 | US-042; stakeholder clarification | V1 | T-044 (test) | None | Draft / not run |

## Stories to requirements and disposition

V1 entries map to proposed requirements. Later stories remain visible without a V1 commitment. US-018 correction remains limited by feasibility; no distributed rollback or guaranteed cancellation is implied.

| Story | Priority | Intent summary | Requirements / disposition |
|---|---|---|---|
| US-001 | V1 | As a human operator, I want a single consolidated view of requests from all my agents | SYS-001 |
| US-002 | V1 | As a human operator, I want to see only actionable information by default | SYS-001 |
| US-003 | V1 | As a human operator, I want requests ordered by importance | SYS-002, SYS-038 |
| US-004 | V1 | As a human operator, I want to see a concise explanation of each request | SYS-003 |
| US-005 | V1 | As a human operator, I want to inspect supporting context on demand | SYS-004 |
| US-006 | V1 | As a human operator, I want to defer a request without losing it | SYS-005 |
| US-007 | V2 | As a human operator, I want to identify my primary objective or "big rock" | Deferred; no V1 requirement |
| US-008 | V2 | As a human operator, I want low-priority requests to resurface periodically | Deferred; no V1 requirement |
| US-009 | V2 | As a human operator, I want related or duplicate requests consolidated | Deferred; no V1 requirement |
| US-010 | V2 | As a human operator, I want to review lower-priority work when higher-priority work is exhausted | Deferred; no V1 requirement |
| US-011 | V1 | As an agent, I want to present a question with any reasonable number of suggested answers | SYS-006 |
| US-012 | V1 | As an agent, I want to identify my recommended answer and explain why | SYS-007 |
| US-013 | V1 | As a human operator, I want the recommended answer presented first and selected by default | SYS-008 |
| US-014 | V1 | As a human operator, I want to choose a custom answer when none of the suggested answers is appropriate. | SYS-006, SYS-009 |
| US-015 | V1 | As a human operator, I want simple approval and denial requests to require minimal interaction. | SYS-010 |
| US-016 | V1 | As an agent, I want to receive the selected answer in a structured form | SYS-011, SYS-036 |
| US-017 | V2 | As a human operator, I want to revise a pending decision request or ask for clarification without losing its context. | Deferred; no V1 requirement |
| US-018 | V2 | As a human operator, I want to correct an accidental response when doing so can still influence the recipient's actions, | Deferred; no V1 requirement |
| US-019 | V1 | As a human operator, I want to send instructions to my agents from a central interface | SYS-012 |
| US-020 | V1 | As an agent, I want to communicate with a particular agent or logical group of agents | SYS-012, SYS-013, SYS-035, SYS-040 |
| US-021 | V1 | As an agent, I want to receive messages relevant to me or the groups I participate in | SYS-014, SYS-035, SYS-040 |
| US-022 | V1 | As an agent, I want to request a response and wait for it | SYS-015 |
| US-023 | V1 | As an agent, I want to retrieve an outstanding response later | SYS-016 |
| US-024 | V1 | As a human operator, I want to receive completion notifications | SYS-017 |
| US-025 | V1 | As a human operator, I want completion notifications to identify the relevant work and its results | SYS-018 |
| US-026 | V2 | As an agent, I want to exchange requests and responses directly with other agents | Deferred; no V1 requirement |
| US-028 | V1 | As a human operator, I want to navigate the queue using only my keyboard | SYS-010, SYS-019 |
| US-029 | V1 | As a human operator, I want consistent keyboard shortcuts for selection, confirmation, deferral, and navigation | SYS-010, SYS-020 |
| US-030 | V1 | As a human operator, I want the interface to update as new requests arrive | SYS-021 |
| US-031 | V1 | As a human operator, I want the interface to work inside an ordinary terminal or tmux pane | SYS-022 |
| US-032 | V2 | As a human operator, I want configurable keybindings and filtering | Deferred; no V1 requirement |
| US-033 | V2 | As a human operator, I want to move quickly between an overview and a detailed request | Deferred; no V1 requirement |
| US-034 | V1 | As an agent developer, I want to interact with the system using a documented CLI | SYS-023 |
| US-035 | V1 | As an agent, I want a portable skill explaining the protocol and command conventions | SYS-024 |
| US-036 | V1 | As a human operator, I want messages and responses to survive crashes and restarts | SYS-025, SYS-036 |
| US-037 | V1 | As an agent, I want failed or retried submissions to avoid producing duplicate requests | SYS-026, SYS-036, SYS-041, SYS-042 |
| US-038 | V1 | As a human operator, I want to review the history of a request, including related messages, decisions, and outcomes, | SYS-027 |
| US-039 | V1 | As a human operator, I want the daemon to operate independently of the TUI | SYS-028 |
| US-040 | V1 | As a human operator, I want to start the system with minimal setup | SYS-029 |
| US-041 | Future | As a human operator, I want to identify sources of excessive or unhelpful messages | Deferred; no V1 requirement |
| US-042 | V1 | As a human operator, I want agents operating on different machines to participate in the same communication system | SYS-030, SYS-044 |
| US-043 | Future | As an agent, I want to discover which participants or groups can handle a particular request | Deferred; no V1 requirement |
| US-044 | Future | As an agent, I want to exchange structured requests and replies with other agents | Deferred; no V1 requirement |
| US-045 | Future | As an operator, I want to configure permissions for agents and groups | Deferred; no V1 requirement |
| US-046 | Future | As an operator, I want visibility into unresolved inter-agent requests and communication failures | Deferred; no V1 requirement |
| US-047 | Future | As an agent, I want messages intended for temporarily unavailable recipients to remain deliverable | Deferred; no V1 requirement |
| US-048 | Future | As an operator, I want to connect alternative interfaces, such as web and graphical clients, | Deferred; no V1 requirement |
| US-049 | V1 | As a human operator, I want to identify the agent and work context associated with a message | SYS-031 |
| US-050 | V2 | As a human operator, I want to navigate directly from a message to its originating work environment, when available, | Deferred; no V1 requirement |
| US-051 | V1 | As an agent, I want to associate messages and requests with related activity | SYS-032 |
| US-052 | V2 | As a human operator, I want to review a chronological history of related activity across multiple agents and sessions | Deferred; no V1 requirement |
| US-053 | V1 | As an agent operating across a network, I want to recover relevant pending communications after losing connectivity | SYS-033 |
| US-054 | Future | As a human operator, I want communication between machines to be restricted to authorized participants | SYS-034, SYS-035 |
| US-055 | Future | Multiple users share a deployment with separated communications. | SYS-037 |
