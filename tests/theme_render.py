#!/usr/bin/env python3
"""Render two xterm palettes in Xvfb; requires xterm, Xvfb, import and Pillow."""
import os
import subprocess
import tempfile
from pathlib import Path

from PIL import Image
from support import BINARY, Service


def main() -> None:
    with tempfile.TemporaryDirectory(prefix="aviary-themes-") as directory, Service() as service:
        service.call({"op": "submit", "request": {
            "identity": "theme-test", "agent": "agent", "question": "Inspect theme contrast",
            "options": ["Approve", "Hold"], "recommendation": 0, "reason": "Ready",
        }})
        for name, bg, fg, accent in (
            ("dark", "#101018", "#eeeeee", "#30d6d0"),
            ("light", "#f7f5ec", "#202020", "#005c78"),
        ):
            output = Path(directory) / f"{name}.png"
            script = '''xrdb -load /dev/null
xterm -geometry 100x24+0+0 -fa monospace -fs 12 -bg "$1" -fg "$2" \\
  -xrm "*.color6: $3" -xrm "*.color14: $3" -xrm "*.pointerColorBackground: $1" \\
  -e "$4" tui "$5" &
pid=$!
sleep 2
import -window root "$6"
kill "$pid"
wait "$pid" 2>/dev/null || true
'''
            env = dict(os.environ)
            env.pop("NO_COLOR", None)
            env["TERM"] = "xterm"
            subprocess.run([
                "xvfb-run", "-a", "-s", "-screen 0 1100x650x24", "sh", "-c", script,
                "theme", bg, fg, accent, str(BINARY), service.address, str(output),
            ], env=env, check=True, timeout=15)
            color = tuple(bytes.fromhex(accent[1:]))
            with Image.open(output) as image:
                assert image.convert("RGB").getcolors(1100 * 650), "unexpected image colors"
                count = sum(1 for pixel in image.convert("RGB").getdata() if pixel == color)
                assert count > 100, f"{name} accent missing: {count} pixels"
        print("Both xterm palettes rendered their configured ANSI cyan")


if __name__ == "__main__":
    main()
