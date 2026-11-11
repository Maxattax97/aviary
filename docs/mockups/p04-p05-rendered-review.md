# Rendered terminal screens for operator review

Captured from the local terminal client in an isolated tmux server with synthetic data. These are static screen captures; they do not demonstrate interactive behavior or every theme.

## Decisions, 100 × 24

```text
Aviary  Active decisions | notices 1 | ? help
┌Decisions────────────────────────┐┌Decision #1 [FOCUS]────────────────────────────────────────────┐
│> #1 Deploy staging release?     ││From: builder                                                  │
│  #2 Select test target          ││Question: Deploy staging release?                              │
│                                 ││importance 0                                                   │
│                                 ││Explanation: Smoke tests passed.                               │
│                                 ││Answers:                                                       │
│                                 ││> 1. Deploy (recommended: Tests passed)                        │
│                                 ││  2. Hold                                                      │
│                                 ││  3. Custom answer                                             │
│                                 ││Enter confirms answer to 1                                     │
│                                 ││                                                               │
│                                 ││                                                               │
│                                 ││                                                               │
│                                 ││                                                               │
│                                 ││                                                               │
│                                 ││                                                               │
│                                 ││                                                               │
│                                 ││                                                               │
│                                 ││                                                               │
│                                 ││                                                               │
└─────────────────────────────────┘└───────────────────────────────────────────────────────────────┘

Tab focus · ↑↓ · Enter confirms · ?
```

## Inbox, 100 × 24

```text
Aviary  Operator inbox | notices 1 | ? help
┌Messages─────────────────────────┐┌Message #1 [FOCUS]─────────────────────────────────────────────┐
│> #1 builder                     ││From: builder                                                  │
│                                 ││Work: Release                                                  │
│                                 ││Message: Staging checks are ready.                             │
│                                 ││Activity: (none supplied)                                      │
│                                 ││                                                               │
│                                 ││H history                                                      │
│                                 ││                                                               │
│                                 ││                                                               │
│                                 ││                                                               │
│                                 ││                                                               │
│                                 ││                                                               │
│                                 ││                                                               │
│                                 ││                                                               │
│                                 ││                                                               │
│                                 ││                                                               │
│                                 ││                                                               │
│                                 ││                                                               │
│                                 ││                                                               │
│                                 ││                                                               │
└─────────────────────────────────┘└───────────────────────────────────────────────────────────────┘

Tab focus · ↑↓ move · PgUp/PgDn · ?
```

## Notices, 100 × 24

```text
Aviary  Completion notices | notices 1 | ? help
┌Notices──────────────────────────┐┌Notice #1 [FOCUS]──────────────────────────────────────────────┐
│> #1 Build release               ││From: builder                                                  │
│                                 ││Work: Build release                                            │
│                                 ││Result: Build completed.                                       │
│                                 ││Activity: (none supplied)                                      │
│                                 ││                                                               │
│                                 ││d dismiss notice #1                                            │
│                                 ││                                                               │
│                                 ││                                                               │
│                                 ││                                                               │
│                                 ││                                                               │
│                                 ││                                                               │
│                                 ││                                                               │
│                                 ││                                                               │
│                                 ││                                                               │
│                                 ││                                                               │
│                                 ││                                                               │
│                                 ││                                                               │
│                                 ││                                                               │
│                                 ││                                                               │
└─────────────────────────────────┘└───────────────────────────────────────────────────────────────┘

Tab focus · ↑↓ move · d dismiss · ?
```

## Decisions, 40 × 12

```text
Aviary Active decisions N:1
┌Decisions─────────────────────────────┐
│> #1 Deploy staging release?          │
│  #2 Select test target               │
└──────────────────────────────────────┘
┌Decision #1 [FOCUS] ↑↓────────────────┐
│Answers:                              │
│> 1. Deploy (recommended: Tests passed│
│)                                     │
└──────────────────────────────────────┘

Tab focus · ↑↓ · Enter confirms · ?
```

## Color scheme examples

These xterm captures use the same synthetic decision and different foreground,
background and ANSI cyan settings. They show the selected theme colors in the
focused border and header; the text focus label and selection marker also remain.

![Dark terminal palette](p04-theme-dark.png)

![Light terminal palette](p04-theme-light.png)
