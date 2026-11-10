#!/usr/bin/env python3
"""Check the two-machine exercise orchestration on loopback only."""

import json
import select
import subprocess
import sys
import uuid
from pathlib import Path

from support import BINARY, Service
from terminal_reconnect import restart, stop


def run_exchange(service: Service, *, expect_restart: bool) -> None:
    script = Path(__file__).with_name("two_machine.py")
    token = uuid.uuid4().hex
    common = [service.address, token, "--binary", str(BINARY), "--timeout", "60"]
    command = [sys.executable, str(script), "origin", *common]
    if expect_restart:
        command.append("--expect-restart")
    origin = subprocess.Popen(
        command, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True
    )
    try:
        assert origin.stdout is not None

        def stage(expected: str) -> dict:
            assert select.select([origin.stdout], [], [], 35)[
                0
            ], f"origin did not report {expected}"
            result = json.loads(origin.stdout.readline())
            assert result["stage"] == expected, result
            return result

        ready = stage("ready")
        if expect_restart:
            stop(service)
            stage("unavailable")
            restart(service)
            recovered = stage("recovered")
            assert recovered["request_id"] == ready["request_id"]
            assert recovered["decision_id"] == ready["decision_id"]
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
        assert result["request_id"] == peer_result["request_id"] == ready["request_id"]
        assert result["reply_id"] == peer_result["reply_id"]
        assert (
            result["decision_id"] == peer_result["decision_id"] == ready["decision_id"]
        )
    finally:
        if origin.poll() is None:
            origin.kill()
            origin.communicate()


def main() -> None:
    with Service() as service:
        run_exchange(service, expect_restart=False)
        run_exchange(service, expect_restart=True)
    print("Two-machine exercise coordinated on loopback, including service restart")


if __name__ == "__main__":
    main()
