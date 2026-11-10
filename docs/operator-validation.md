# Representative operator validation plan

**Status: prepared, not run.** This is a proposed session for V1-23, not evidence that Aviary is ready. A representative operator and a facilitator should review the scenario and the supported platform before running it. Record actual observations and unresolved findings in [verification](verification.md); do not substitute the automated pseudo-terminal tests for a human session.

## Setup

Use an isolated test database and a service on loopback or an explicitly trusted test network. Record the Aviary commit, OS, terminal and tmux versions if used, pane dimensions, and whether participants and service are on separate machines. The facilitator creates a channel with `operator` and two distinct agent labels; use fresh submission identities and an activity reference unique to the session. The [CLI reference](cli.md) gives the exact operation shapes and retry rules. Keep session data non-sensitive and record observations without participant identifiers unless needed and agreed.

Use real agent workflows where available. If the facilitator simulates an agent or uses a waiting CLI as a stand-in, record that fact: the session can demonstrate the operator interface but cannot establish that a real agent resumed useful work. Keep the facilitator's expected answers and test setup out of the operator's view until after the tasks.

## Comparable work

Prepare two equivalent, low-risk task sets, each with two agents. In each set, one agent requests a timely approval with a recommendation and reason; the other supplies a lower-priority decision with an explanation and context that the operator must inspect before choosing. Include an ordinary progress message and a completion result, each with distinct, recognizable content. Use different content in the two sets so that a remembered answer does not determine the second task. Agree the expected correct responses with the facilitator beforehand, without assuming a timing threshold.

Run one set using the operator's normal way of monitoring agent sessions and the other using Aviary. If more than one session is possible, alternate the order to reduce learning effects. Record the baseline workflow, differences between task sets, interruptions, and whether agents are real or simulated. This is a small comparative observation, not a statistically representative performance study.

## Aviary session

1. With the interface open, have both agents submit the prepared decisions, the routine message, and the completion notice. The operator identifies what requires action and chooses the higher-priority request. The facilitator confirms no answer was submitted merely by display or navigation.
2. The operator reads the recommendation and reason, accepts the intended recommended answer, and the requesting agent retrieves the structured response and continues its work. Record the number of confirmation actions after the request is in focus, any unintended action, and what the agent actually did after receiving the answer.
3. The operator inspects the other request's context, defers it, closes and reopens the interface, finds it in the deferred queue, restores it, and submits the intended answer. The agent retrieves the response after a requester interruption or restart. Record whether the operator could find the deferred work and whether the response enabled continuation.
4. The operator finds the ordinary message in the inbox, inspects the completion notice and result, and sends an instruction to a selected agent or group. Verify the intended recipients retrieve the instruction. Record any confusion between decisions, messages and notices.
5. Repeat relevant navigation in each terminal and pane configuration proposed for support, including tmux where applicable. Record any clipped content, inaccessible controls, keyboard-only difficulties, or reliance on facilitator help. Do not interpret a single pane size as a general support guarantee.

Give the operator the same task goals for both workflows. Avoid coaching individual keys during the timed or observed portion; note any documentation or assistance used. A facilitator may stop or correct the session if it cannot proceed, recording the interruption and result rather than silently treating the task as passed.

## Observation record

For each task set, record task completion and errors, time to identify and answer actionable requests, number of agent-session or window switches, requests deferred and recovered, confirmation actions, agent response and actual continuation, and messages or completion results missed or mistaken for decisions. Capture operator comments on confidence, attention, context switching and friction. Note order effects, any deviations, environment limitations and whether the same operator performed both sets. Use times and counts as observations; no latency or improvement threshold has been approved.

Summarize the operator's assessment and each unresolved issue with severity, reproduction conditions and a proposed next action. Review findings and agree acceptance thresholds and the platform matrix with the stakeholder before claiming V1 acceptance. Separately run the requirement tests and the two-machine and OS-crash checks; this session does not establish those results.
