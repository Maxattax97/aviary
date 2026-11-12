#!/usr/bin/env python3
"""Check the two-machine exercise orchestration on loopback only."""

import json
import select
import subprocess
import sys
import time
import uuid
from pathlib import Path

from support import BINARY, Service, Terminal
from terminal_reconnect import restart, stop


def run_exchange(service: Service, *, expect_restart: bool, operator_answer: bool = False) -> None:
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
        if operator_answer:
            def waiter():
                return subprocess.Popen(
                    [str(BINARY), "wait", service.address, str(ready["decision_id"])],
                    stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
                )

            first_waiter = waiter()
            restarted_waiter = None
            peer = subprocess.Popen(
                [sys.executable, str(script), "peer", *common, "--operator-answer"],
                stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
            )
            try:
                time.sleep(0.5)
                assert first_waiter.poll() is None
                first_waiter.kill()
                first_waiter.communicate(timeout=5)
                restarted_waiter = waiter()
                time.sleep(0.5)
                assert restarted_waiter.poll() is None
                assert peer.stdout is not None
                assert select.select([peer.stdout], [], [], 20)[0]
                waiting = json.loads(peer.stdout.readline())
                assert waiting["stage"] == "awaiting_operator"
                assert waiting["decision_id"] == ready["decision_id"]
                assert service.call({"op": "get", "id": ready["decision_id"]})["answer"] is None
                with Terminal(service) as terminal:
                    terminal.expect(waiting["question"][:-1])
                    terminal.send("\r")
                    terminal.expect("No decisions")
                    terminal.quit()
                waiter_output, waiter_error = restarted_waiter.communicate(timeout=20)
                assert restarted_waiter.returncode == 0, waiter_error
                assert json.loads(waiter_output) == {
                    "id": ready["decision_id"],
                    "answer": {"type": "suggested", "index": 1, "value": "Yes"},
                }
                peer_output, peer_error = peer.communicate(timeout=20)
                assert peer.returncode == 0, peer_error
                peer_result = json.loads(peer_output)
            finally:
                for process in (first_waiter, restarted_waiter):
                    if process is not None and process.poll() is None:
                        process.kill()
                        process.communicate()
                if peer.poll() is None:
                    peer.kill()
                    peer.communicate()
        else:
            peer = subprocess.run(
                [sys.executable, str(script), "peer", *common],
                capture_output=True, text=True, timeout=20, check=True,
            )
            peer_result = json.loads(peer.stdout)
        remaining, errors = origin.communicate(timeout=20)
        assert origin.returncode == 0, errors
        result = json.loads(remaining)
        assert result["stage"] == peer_result["stage"] == "complete"
        assert result["waiter_recovered"] is True
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
        run_exchange(service, expect_restart=False, operator_answer=True)
    print("Two-machine exercise coordinated on loopback, including restart and terminal answer")


if __name__ == "__main__":
    main()
