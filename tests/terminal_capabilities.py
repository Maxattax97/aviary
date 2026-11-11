#!/usr/bin/env python3
"""Exercise basic-color and monochrome workflows directly and in tmux."""
import os
import re
import subprocess
import tempfile
import time
from pathlib import Path

from support import BINARY, Service, Terminal


def submit(service, identity):
    return service.call({"op": "submit", "request": {"identity": identity, "agent": "agent",
        "question": "Capability decision", "options": ["Yes", "No"], "recommendation": 0, "reason": "Ready"}})["id"]


def check_colors(output, monochrome):
    assert not re.search(rb"\x1b\[(?:38|48);(?:5|2);", output), f"unexpected colors: {set(re.findall(rb'\x1b\[[0-9;]*m', output))}"
    if monochrome:
        assert not re.search(rb"\x1b\[(?:3[0-7]|9[0-7])(?:;|m)", output), "unexpected color in NO_COLOR mode"
    else:
        assert b"\x1b[36" in output, "basic cyan accent not emitted"


def direct(monochrome):
    with Service() as service:
        decision = submit(service, f"direct-{monochrome}")
        env = {**os.environ, "TERM": "xterm"}
        if monochrome:
            env["NO_COLOR"] = "1"
        else:
            env.pop("NO_COLOR", None)
        with Terminal(service, env=env) as terminal:
            chunks = []
            feed = terminal._feed_screen
            def capture(chunk):
                chunks.append(chunk)
                feed(chunk)
            terminal._feed_screen = capture
            screen = terminal.expect("Capability decision")
            assert b"[Decision]" in screen and b"> 1. Yes" in screen
            terminal.send("?")
            terminal.expect("Keyboard help")
            terminal.send("\x1b")
            terminal.expect("Capability decision")
            terminal.send("m")
            terminal.expect("Destination: all")
            terminal.send("Unavailable channel\r")
            terminal.expect("Rejected:")
            assert service.call({"op": "get", "id": decision})["answer"] is None
            terminal.send("\x1b")
            terminal.expect("Discard this")
            terminal.send("y")
            terminal.expect("Capability decision")
            terminal.send("\r")
            service.wait_for(lambda: service.call({"op": "get", "id": decision})["answer"] is not None, "direct confirmed decision")
            terminal.expect("Action accepted")
            terminal.quit()
            check_colors(b"".join(chunks), monochrome)


def tmux_case(monochrome):
    with tempfile.TemporaryDirectory(prefix="aviary-capabilities-") as directory, Service() as service:
        decision = submit(service, f"tmux-{monochrome}")
        socket = str(Path(directory) / "tmux.sock")
        output = Path(directory) / "pane.raw"
        def tmux(*args, check=True):
            return subprocess.run(["tmux", "-f", "/dev/null", "-S", socket, *args], text=True, capture_output=True, timeout=5, check=check)
        def wait_for(marker):
            deadline = time.monotonic() + 8
            while time.monotonic() < deadline:
                screen = tmux("capture-pane", "-p", "-t", "aviary:", check=False)
                if screen.returncode == 0 and marker in screen.stdout:
                    return screen.stdout
                time.sleep(0.05)
            raise AssertionError(f"missing {marker!r}: {screen.stdout}")
        env = ["env", "TERM=xterm", "NO_COLOR=1"] if monochrome else ["env", "-u", "NO_COLOR", "TERM=xterm"]
        try:
            tmux("new-session", "-d", "-s", "aviary", "-x", "100", "-y", "24", *env, str(BINARY), "tui", service.address)
            screen = wait_for("Capability decision")
            assert "[Decision]" in screen and "> 1. Yes" in screen
            tmux("pipe-pane", "-t", "aviary:", f"cat > {output}")
            tmux("send-keys", "-t", "aviary:", "?")
            wait_for("Keyboard help")
            tmux("send-keys", "-t", "aviary:", "Escape")
            wait_for("Capability decision")
            tmux("send-keys", "-t", "aviary:", "m")
            wait_for("Destination: all")
            tmux("send-keys", "-t", "aviary:", "-l", "Unavailable channel")
            tmux("send-keys", "-t", "aviary:", "Enter")
            wait_for("Rejected:")
            assert service.call({"op": "get", "id": decision})["answer"] is None
            tmux("send-keys", "-t", "aviary:", "Escape")
            wait_for("Discard this")
            tmux("send-keys", "-t", "aviary:", "y")
            wait_for("Capability decision")
            tmux("send-keys", "-t", "aviary:", "Enter")
            service.wait_for(lambda: service.call({"op": "get", "id": decision})["answer"] is not None, "tmux confirmed decision")
            wait_for("Action accepted")
            tmux("pipe-pane", "-t", "aviary:")
            check_colors(output.read_bytes(), monochrome)
        finally:
            tmux("kill-server", check=False)


def main():
    for monochrome in (False, True):
        direct(monochrome)
        tmux_case(monochrome)
    print("Basic-color and monochrome direct/tmux workflows passed")


if __name__ == "__main__":
    main()
