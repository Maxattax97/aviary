#!/usr/bin/env python3
"""Exercise the local decision slice across requester and service restarts."""

import json
from pathlib import Path
import sqlite3
import subprocess
import time

from support import BINARY, Service, Terminal
from terminal_reconnect import restart, stop


def main() -> None:
    with Service() as service:
        request = {
            "identity": "decision-slice",
            "agent": "local-agent",
            "question": "Approve local slice?",
            "options": ["No", "Yes"],
            "recommendation": 1,
            "reason": "Ready",
        }
        submit = {"op": "submit", "request": request}
        request_id = service.call(submit)["id"]
        expected = {
            "id": request_id,
            "answer": {"type": "suggested", "index": 1, "value": "Yes"},
        }

        def waiter() -> subprocess.Popen:
            return subprocess.Popen(
                [str(BINARY), "wait", service.address, str(request_id)],
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
            )

        first = waiter()
        current = None
        try:
            time.sleep(0.75)
            assert first.poll() is None, "wait exited before an answer"
            assert service.call({"op": "get", "id": request_id})["answer"] is None
            first.kill()
            first.communicate(timeout=5)
            current = waiter()
            time.sleep(0.75)
            assert current.poll() is None, "restarted wait exited before an answer"
            with Terminal(service) as terminal:
                terminal.expect("> 1. Yes (recommended: Ready)")
                assert service.call({"op": "get", "id": request_id})["answer"] is None
                terminal.send("\r")
                output, error = current.communicate(timeout=10)
                assert current.returncode == 0, error
                assert json.loads(output) == expected
                terminal.expect("No active requests")
                terminal.quit()
            stop(service)
            restart(service)
            assert service.call(submit)["id"] == request_id
            changed = {"op": "submit", "request": {**request, "question": "Different"}}
            conflict = service.run(changed)
            assert conflict.returncode != 0 and "identity conflict" in conflict.stderr
            recovered = subprocess.run(
                [str(BINARY), "wait", service.address, str(request_id)],
                capture_output=True,
                text=True,
                timeout=15,
                check=True,
            )
            assert json.loads(recovered.stdout) == expected
            history = service.call(
                {
                    "op": "history_page",
                    "kind": "decision",
                    "id": request_id,
                    "section": "decisions",
                }
            )
            assert len(history["decisions"]) == 1
            assert history["decisions"][0]["answer"] == expected["answer"]
            with sqlite3.connect(
                Path(service.directory.name) / "state.sqlite"
            ) as connection:
                count = connection.execute(
                    "SELECT COUNT(*) FROM requests WHERE identity = ?",
                    (request["identity"],),
                ).fetchone()[0]
            assert count == 1
        finally:
            for process in (first, current):
                if process is not None and process.poll() is None:
                    process.kill()
                    process.communicate(timeout=5)
    print(
        "Local decision: restarted waiter, terminal confirmation, service restart and retry passed"
    )


if __name__ == "__main__":
    main()
