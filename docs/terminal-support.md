# Terminal and platform verification matrix

This records exercised configurations and remaining targets; it is not a claim that every version of a listed platform or terminal is supported. V1 platform and accessibility acceptance still requires agreement and actual installation and workflow tests on each chosen platform.

| Configuration | Local evidence | Limits |
| --- | --- | --- |
| Nobara Linux 44, x86-64, kernel 7.2.11-200, glibc 2.43, Rust 1.90.0 | Existing toolchain build, local service, CLI and PTY/tmux tests | Not a clean-OS installation or coverage of other Linux releases |
| Direct pseudo-terminal, `TERM=xterm`, 80×24, basic ANSI colors | Selection, help, rejection and explicit decision, including captured output | PTY simulation is not a physical terminal or an accessibility audit |
| Direct pseudo-terminal, `TERM=xterm`, `NO_COLOR=1`, 80×24 | Same workflow and non-color markers; captured output checked | Does not establish contrast or every screen and terminal |
| Direct pseudo-terminal, inherited `TERM=tmux-256color`, 80×24 | Synthetic SGR wheel events in decision, inbox, notice, expanded context and history views; both boundaries in each view, non-focused panes, pane border, header/footer controls, keyboard PageUp and keyboard-only `TERM=dumb` path | PTY injection proves app handling after report delivery, not physical terminal mouse support; help/search/overview wheel input not exercised |
| tmux 3.7c, `TERM=xterm`, basic colors and `NO_COLOR=1`, 100×24 | Same workflow and captured application output | Other tmux releases and terminal emulators remain untested |
| tmux 3.8 attached PTY, `TERM=xterm-256color`, 100×30, mouse mode on/off | Synthetic SGR wheel events moved focused decision detail and list in both directions through their boundaries; keyboard navigation still worked after turning tmux mouse mode off | One local tmux version and PTY client; physical terminals and mouse-report-disabled behavior remain unverified |
| tmux 3.7c, 40×12 and 100-column panes | Decision, preview, detail, history and resize regressions | Selected examples and Unicode only |
| XTerm 411 under Xvfb, 100×24, two configured palettes | Dark and light palettes rendered their configured ANSI cyan | Two sampled palettes; no universal contrast guarantee |
| musl Linux, macOS, BSD, Herdr | Targeted for future validation | No installation or complete workflow evidence in this matrix |

The current minimum interactive size is 40 columns × 12 rows; smaller panes request resize rather than accept normal actions. The interface uses terminal-controlled ANSI palette colors: basic cyan for focus and selection, and basic yellow for warning/rejection text. The output adapter maps the indexed forms emitted by the rendering library for those colors to basic SGR sequences. `NO_COLOR` and `TERM=dumb` suppress color emphasis; `>` and bracketed focus titles remain non-color cues. Basic-color and monochrome test captures check for absence of indexed and RGB color sequences in the exercised workflows. Other terminal capabilities, theme contrast, screen readers, Unicode width conventions and accessibility needs remain to be validated with operators.

Keyboard operation remains available without mouse input. The interface uses alternate-screen, cursor positioning and raw keyboard input; mouse focus and selection are optional. Temporary navigation and drafts are process-local; they do not survive terminal exit. A resize or terminal/emulator failure outside exercised cases is not proven safe by this matrix.

## In-app text selection and clipboard

In a queue view, press and hold the left mouse button in the detail pane's text area, drag across the text, and release to copy. The highlight and copied text stay within the detail pane's content area, even if the pointer crosses into the decision list; pane borders are excluded. The copy runs independently of the terminal event loop so keyboard and Ctrl-C remain available while a clipboard helper owns the selection. On Wayland, Aviary uses `wl-copy`; on X11 it tries `xclip` and `xsel`. If no helper succeeds, Aviary sends an OSC 52 request through the terminal; that fallback depends on terminal support and mux clipboard policy.

Manual check in the target setup: open a decision with unique multiline text visible in the right detail column, run Aviary in `st` inside tmux (and repeat with Herdr nested inside tmux if used), drag across the text and into the left list, then release and paste with `Shift+Insert` or `Alt+v` in `st`. Confirm the clipboard contains only detail text without border characters, the UI still responds to navigation, and Ctrl-C exits. A shell check is `wl-paste --no-newline` when the host uses Wayland.
