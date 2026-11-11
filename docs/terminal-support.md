# Terminal and platform verification matrix

This records exercised configurations and remaining targets; it is not a claim that every version of a listed platform or terminal is supported. V1 platform and accessibility acceptance still requires agreement and actual installation and workflow tests on each chosen platform.

| Configuration | Local evidence | Limits |
| --- | --- | --- |
| Nobara Linux 44, x86-64, kernel 7.2.11-200, glibc 2.43, Rust 1.90.0 | Existing toolchain build, local service, CLI and PTY/tmux tests | Not a clean-OS installation or coverage of other Linux releases |
| Direct pseudo-terminal, `TERM=xterm`, 80×24, basic ANSI colors | Selection, help, rejection and explicit decision, including captured output | PTY simulation is not a physical terminal or an accessibility audit |
| Direct pseudo-terminal, `TERM=xterm`, `NO_COLOR=1`, 80×24 | Same workflow and non-color markers; captured output checked | Does not establish contrast or every screen and terminal |
| tmux 3.7c, `TERM=xterm`, basic colors and `NO_COLOR=1`, 100×24 | Same workflow and captured application output | Other tmux releases and terminal emulators remain untested |
| tmux 3.7c, 40×12 and 100-column panes | Decision, preview, detail, history and resize regressions | Selected examples and Unicode only |
| XTerm 411 under Xvfb, 100×24, two configured palettes | Dark and light palettes rendered their configured ANSI cyan | Two sampled palettes; no universal contrast guarantee |
| musl Linux, macOS, BSD, Herdr | Targeted for future validation | No installation or complete workflow evidence in this matrix |

The current minimum interactive size is 40 columns × 12 rows; smaller panes request resize rather than accept normal actions. The interface uses terminal-controlled ANSI palette colors: basic cyan for focus and selection, and basic yellow for warning/rejection text. The output adapter maps the indexed forms emitted by the rendering library for those colors to basic SGR sequences. `NO_COLOR` and `TERM=dumb` suppress color emphasis; `>` and bracketed focus titles remain non-color cues. Basic-color and monochrome test captures check for absence of indexed and RGB color sequences in the exercised workflows. Other terminal capabilities, theme contrast, screen readers, Unicode width conventions and accessibility needs remain to be validated with operators.

Keyboard operation remains available without mouse input. The interface uses alternate-screen, cursor positioning and raw keyboard input; mouse focus and selection are optional. Temporary navigation and drafts are process-local; they do not survive terminal exit. A resize or terminal/emulator failure outside exercised cases is not proven safe by this matrix.
