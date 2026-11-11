# P-04 / P-05 terminal layout for operator review

**Status: operator reviewed on 2026-11-10; not yet implemented.** The examples show content and
focus, not exact terminal colors. They use the existing `all` group-message
behavior; they do not propose shared-work queue or deferral policy changes.

## Wide pane: decisions (illustrative 80 × 24)

```text
 Aviary  Decisions [active]    Inbox (i)  Notices (n)               ? help
+-- Decisions [FOCUS] --------+-- Decision #42 -------------------------------+
| > #42  Deploy release       | From: build-agent                            |
|   #39  Choose test target   | Question:                                    |
|   #31  Approve migration    |   Deploy release to staging?                 |
|                             | Explanation:                                 |
|   Loading more...           |   Smoke tests passed.                        |
|                             | Context:                                     |
|                             |   Build 281; see linked activity.             |
|                             |                                              |
|                             | Answers:                                     |
|                             | > (selected) Deploy [recommended]            |
|                             |   Hold                                       |
|                             |   Custom answer                              |
|                             |                                              |
|                             | Enter confirms selected answer for #42       |
+-----------------------------+----------------------------------------------+
 Tab focus  ↑/↓ move  m instruction  ? help
```

The list focus selects records with Up/Down (also k/j); Tab moves to details.
In decision details Up/Down selects an answer and Enter confirms it; reading
long details uses Page Up/Page Down, with visible continuation markers. The
selected record and answer use text markers as well as highlighting. Context,
custom answers, priority, deferral, history, inbox and notices remain reachable;
help describes the complete bindings. Navigating or moving focus never submits.
Inbox and notices use the same list/detail framing, with labeled sender/work/body
or work/result fields arranged vertically; their details scroll by visible lines.
Notice dismissal and inbox reply remain explicit actions on the selected item.

## Inbox and notices (wide panes)

```text
 Aviary  Inbox                                                     ? help
+-- Messages [FOCUS] ----------+-- Message #105 ------------------------------+
| > #105  build-agent          | From: build-agent                            |
|   #102  test-agent           | Work: staging release                        |
|   #98   deploy-agent         | Message:                                     |
|                              |   Smoke tests passed.                        |
|   More available ↓           |                                              |
|                              | r reply to #105  H history                   |
+------------------------------+----------------------------------------------+
 Tab focus  ↑/↓ move  i decisions  ? help

 Aviary  Notices                                                   ? help
+-- Notices -------------------+-- Notice #27 [FOCUS] ------------------------+
| > #27  Release complete      | From: deploy-agent                           |
|   #24  Tests complete        | Work: staging release                        |
|                              | Result:                                      |
|                              |   Deployment completed.                      |
|                              |                                              |
|                              | d dismiss notice #27                         |
+------------------------------+----------------------------------------------+
 Tab focus  ↑/↓ scroll  n decisions  ? help
```

The list indicates more records without asserting an exact total; selecting an
item updates its details. When a fetch fails, the list keeps the last usable
content and displays `Could not load more; retry` instead of an end marker.

## Narrow pane: stacked focus (illustrative 40 × 12)

```text
 Aviary  Decisions             ? help
+-- Decisions [FOCUS] -----------------+
| > #42 Deploy release                 |
|   #39 Choose test target             |
+--------------------------------------+
+-- Decision #42 ----------------------+
| From: build-agent                    |
| Question: Deploy release to staging? |
| ↓ more                               |
+--------------------------------------+
 Tab focus  ↑/↓ move  ? help
```

On a narrow pane, list and details stack; focus remains explicit. Tab moves to
details, where the content and choices can be scrolled and operated. A smaller
terminal that cannot fit the tested minimum should show a resize instruction
instead of silently hiding actions. The proposed initial verification target is
40 × 12; it is not a claim of support until tested.

## Instruction composer

```text
+-- Compose instruction [FOCUS] ---------------------------------------------+
| Destination: all · group (existing channel membership)                      |
| Effect: send this instruction to the group's other members                  |
|                                                                             |
| Please inspect the staging deployment and report the result.                |
|                                                                             |
| Ctrl-S send  Esc discard  Ctrl-G help                                       |
+----------------------------------------------------------------------------+
```

Ctrl-S is the explicit send action directly from the editor; editing and
navigation do not send. The destination and effect stay visible, including
when text scrolls. The full exact draft is inspectable in the editor. A pending
send shows `Sending — outcome unconfirmed`; it cannot be submitted again while
pending. After acceptance, show `Instruction sent` briefly (proposed 4 seconds).
An explicit rejection stays visible with its reason and corrective action. An
unconfirmed result stays visible with `Outcome unknown; retry unchanged`;
only an explicit Ctrl-S retries the unchanged draft and identity. Do not allow
editing that would silently reuse an uncertain identity for different content.
The unconfirmed state also offers a way to temporarily leave the composer while
retaining the locked draft and identity, then return to retry or deliberately
discard it. An identical retry can recover the original ID; a history lookup
by submission identity is not assumed. Exiting Aviary still loses in-memory drafts;
show the identity and warn before discarding an uncertain draft. Esc requests
discard and requires confirmation when the outcome is unconfirmed. The existing channel is not provisioned, reinterpreted or modified by this UI.

## Loading, color, and boundaries

- Fetch bounded backend pages asynchronously when navigation approaches a
  boundary; Up/Down continues across pages without a page-selection command.
  Show loading and fetch errors in the affected region; do not imply a total
  or a complete snapshot when more data may exist. Retain bounded payloads and
  disclose independent offset-page snapshots and possible concurrent shifts.
- Use terminal-default foreground/background and theme-controlled basic ANSI
  colors for accents. Focus labels, borders, selection markers, status words and
  continuation arrows convey meaning without color. Test monochrome and basic
  color, including narrow panes; avoid assuming truecolor.
- Success notifications expire without changing selection or draft; rejection
  and unconfirmed outcomes persist until resolved or dismissed deliberately.
  A failure to refresh is distinct from an action result.

## Review choices

1. Is the list/detail focus model and stacked narrow layout suitable, including
   Up/Down for the focused region, Tab to switch, and Page Up/Down for reading?
2. Is Ctrl-S as the direct send confirmation acceptable, with Enter reserved for
   editing and an unchanged explicit retry after an uncertain outcome?
3. Is a four-second success notification suitable? Errors and uncertain results
   remain until deliberately resolved or dismissed.

The mockup is the reviewed design direction. Actual terminal rendering, keyboard behavior,
accessibility, fetch boundaries and operator acceptance remain unverified.
