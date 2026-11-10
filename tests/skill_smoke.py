#!/usr/bin/env python3
"""Execute the Aviary skill's shell examples against a local test service."""
import json
import os
from pathlib import Path
import re
import socket
import subprocess
import tempfile
import time

root = Path(__file__).resolve().parent.parent
binary = root / "target/debug/aviary"
blocks = re.findall(r"```sh\n(.*?)\n```", (root / "skills/aviary/SKILL.md").read_text(), re.S)
assert len(blocks) == 2
sock = socket.socket()
sock.bind(("127.0.0.1", 0))
port = sock.getsockname()[1]
sock.close()
address = f"127.0.0.1:{port}"
env = dict(os.environ, PATH=f"{binary.parent}:{os.environ.get('PATH', '')}",
           AVIARY_ADDR=address, AVIARY_AGENT="agent-a", AVIARY_CHANNEL="team",
           AVIARY_RECIPIENT="agent-b")
with tempfile.TemporaryDirectory() as directory:
    server = subprocess.Popen([str(binary), "serve", str(Path(directory) / "state.sqlite"), address],
                              stderr=subprocess.DEVNULL)
    try:
        for _ in range(100):
            try:
                with socket.create_connection(("127.0.0.1", port), timeout=.1):
                    break
            except OSError:
                time.sleep(.02)
        else:
            raise RuntimeError("test service failed to start")
        def call(operation):
            return subprocess.run([str(binary), "call", address, json.dumps(operation)],
                                  text=True, capture_output=True, timeout=15)
        channel = call({"op": "create_channel", "channel": {"name": "team", "members": ["agent-a", "agent-b"]}})
        assert channel.returncode == 0, channel.stderr
        env["AVIARY_KEY"] = "skill-decision"
        decision = subprocess.run(["/bin/sh", "-eu", "-c", blocks[0]], env=env, text=True,
                                  capture_output=True, timeout=15, check=True)
        result = [json.loads(line) for line in decision.stdout.splitlines()]
        assert len(result) == 2 and result[0]["id"] == result[1]["id"] and result[1]["answer"] is None
        env["AVIARY_KEY"] = "skill-message"
        message = subprocess.run(["/bin/sh", "-eu", "-c", blocks[1]], env=env, text=True,
                                 capture_output=True, timeout=15, check=True)
        result = [json.loads(line) for line in message.stdout.splitlines()]
        assert result[0]["id"] == result[1]["messages"][0]["id"]
        assert result[1]["messages"][0]["message"]["body"] == "Please review"
        failed = call({"op": "get", "id": 999999})
        assert failed.returncode != 0 and "request not found" in failed.stderr
        print("skill examples: decision, message, retrieval and error status passed")
    finally:
        server.kill()
        server.wait()
