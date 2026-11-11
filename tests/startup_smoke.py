#!/usr/bin/env python3
"""Exercise one-service startup and communication without the interface."""

import json
import socket
import subprocess
import tempfile
import time
from pathlib import Path
from types import SimpleNamespace

from support import BINARY, Terminal


def main() -> None:
    with tempfile.TemporaryDirectory(prefix="aviary-startup-") as directory:
        cwd = Path(directory)
        env = {"HOME": directory, "PATH": "/usr/bin:/bin", "TERM": "xterm-256color"}
        with socket.socket() as listener:
            listener.bind(("127.0.0.1", 0))
            address = f"127.0.0.1:{listener.getsockname()[1]}"
        service = subprocess.Popen(
            [str(BINARY), "serve", "state.sqlite", address],
            cwd=cwd,
            env=env,
            stderr=subprocess.PIPE,
        )

        def call(operation: dict) -> dict:
            result = subprocess.run(
                [str(BINARY), "call", address, json.dumps(operation)],
                cwd=cwd,
                env=env,
                text=True,
                capture_output=True,
                check=True,
                timeout=15,
            )
            return json.loads(result.stdout)

        try:
            deadline = time.monotonic() + 5
            while True:
                if service.poll() is not None:
                    raise AssertionError("service exited before listening")
                try:
                    with socket.create_connection(
                        ("127.0.0.1", int(address.rsplit(":", 1)[1])), timeout=0.1
                    ):
                        break
                except OSError:
                    if time.monotonic() >= deadline:
                        raise TimeoutError("service did not listen")
                    time.sleep(0.02)
            assert (cwd / "state.sqlite").is_file()
            context = SimpleNamespace(address=address)
            with Terminal(context, cwd=cwd, env=env) as terminal:
                terminal.expect("No decisions")
                terminal.quit()
            assert service.poll() is None
            call(
                {
                    "op": "create_channel",
                    "channel": {"name": "ops", "members": ["operator", "worker"]},
                }
            )
            request_id = call(
                {
                    "op": "send_message",
                    "message": {
                        "identity": "startup-request",
                        "channel": "ops",
                        "sender": "operator",
                        "target": {"kind": "participant", "name": "worker"},
                        "body": "Inspect",
                        "expects_reply": True,
                        "context": "Fresh startup",
                        "activity": "startup-activity",
                    },
                }
            )["id"]
            reply_id = call(
                {
                    "op": "reply_message",
                    "reply": {
                        "identity": "startup-reply",
                        "request_id": request_id,
                        "sender": "worker",
                        "body": "Complete",
                    },
                }
            )["id"]
            assert (
                call({"op": "get_message_response", "id": request_id})["response"]["id"]
                == reply_id
            )
            with Terminal(context, cwd=cwd, env=env) as terminal:
                terminal.expect("No decisions")
                terminal.send("i")
                screen = terminal.expect("Complete")
                assert b"From: worker" in screen and b"Fresh startup" in screen
                terminal.send("q")
                terminal.expect("No decisions")
                terminal.quit()
            history = call({"op": "history", "kind": "message", "id": request_id})
            assert [item["id"] for item in history["messages"]] == [
                request_id,
                reply_id,
            ]
            assert service.poll() is None
        finally:
            if service.poll() is None:
                service.terminate()
            try:
                service.wait(timeout=5)
            except subprocess.TimeoutExpired:
                service.kill()
                service.wait(timeout=5)
                raise
            if service.stderr is not None:
                service.stderr.close()
    print(
        "Fresh directory: service continued across terminal closure; request and reply recovered"
    )


if __name__ == "__main__":
    main()
