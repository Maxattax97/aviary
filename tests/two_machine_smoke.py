#!/usr/bin/env python3
"""Check the two-machine exercise orchestration on loopback only."""

import json
import select
import subprocess
import sys
import uuid
from pathlib import Path

from support import BINARY, Service


def main() -> None:
    script = Path(__file__).with_name("two_machine.py")
    with Service() as service:
        token = uuid.uuid4().hex
        common = [service.address, token, "--binary", str(BINARY), "--timeout", "15"]
        origin = subprocess.Popen(
            [sys.executable, str(script), "origin", *common],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )
        try:
            assert origin.stdout is not None
            assert select.select([origin.stdout], [], [], 35)[
                0
            ], "origin did not become ready"
            ready = json.loads(origin.stdout.readline())
            assert ready["stage"] == "ready"
            peer = subprocess.run(
                [sys.executable, str(script), "peer", *common],
                capture_output=True,
                text=True,
                timeout=20,
                check=True,
            )
            peer_result = json.loads(peer.stdout)
            remaining, errors = origin.communicate(timeout=20)
            assert origin.returncode == 0, errors
            result = json.loads(remaining)
            assert result["stage"] == peer_result["stage"] == "complete"
            assert (
                result["request_id"] == peer_result["request_id"] == ready["request_id"]
            )
            assert result["reply_id"] == peer_result["reply_id"]
            assert (
                result["decision_id"]
                == peer_result["decision_id"]
                == ready["decision_id"]
            )
        finally:
            if origin.poll() is None:
                origin.kill()
                origin.communicate()
    print("Two-machine exercise coordinated successfully on loopback only")


if __name__ == "__main__":
    main()
