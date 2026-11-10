#!/usr/bin/env python3
"""Run one half of a deliberately coordinated trusted-network exchange."""

import argparse
import json
import re
import socket
import subprocess
import time


def call(binary: str, address: str, operation: dict) -> dict:
    completed = subprocess.run(
        [binary, "call", address, json.dumps(operation, ensure_ascii=False)],
        capture_output=True,
        text=True,
        check=True,
        timeout=15,
    )
    return json.loads(completed.stdout)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("role", choices=("origin", "peer"))
    parser.add_argument("address", help="explicit address of the trusted test service")
    parser.add_argument(
        "token", help="the same fresh 32-character lowercase hex token on both machines"
    )
    parser.add_argument(
        "--binary", default="aviary", help="path to the matching Aviary executable"
    )
    parser.add_argument(
        "--timeout", type=int, default=120, help="origin wait limit in seconds"
    )
    parser.add_argument(
        "--expect-restart",
        action="store_true",
        help="origin waits for a service outage and verifies retries after recovery",
    )
    args = parser.parse_args()
    if (
        not re.fullmatch(r"[0-9a-f]{32}", args.token)
        or args.timeout <= 0
        or (args.expect_restart and args.role != "origin")
    ):
        parser.error(
            "token must be 32 lowercase hex characters and timeout must be positive"
        )

    token = args.token
    origin, peer = f"origin-{token}", f"peer-{token}"
    channel, identity = f"test-{token}", f"request-{token}"
    reply_body = f"reply {token}"
    host = socket.gethostname()
    if args.role == "origin":
        call(
            args.binary,
            args.address,
            {
                "op": "create_channel",
                "channel": {"name": channel, "members": [origin, peer]},
            },
        )
        decision_operation = {
            "op": "submit",
            "request": {
                "identity": f"decision-{token}",
                "agent": origin,
                "question": f"Proceed {token}?",
                "options": ["No", "Yes"],
                "recommendation": 1,
                "reason": "Test exchange",
            },
        }
        decision_id = call(args.binary, args.address, decision_operation)["id"]
        expected_body = f"request {token} decision {decision_id}"
        request = {
            "identity": identity,
            "channel": channel,
            "sender": origin,
            "target": {"kind": "participant", "name": peer},
            "body": expected_body,
            "expects_reply": True,
            "activity": channel,
        }
        request_operation = {"op": "send_message", "message": request}
        request_id = call(args.binary, args.address, request_operation)["id"]
        print(
            json.dumps(
                {
                    "stage": "ready",
                    "host": host,
                    "request_id": request_id,
                    "decision_id": decision_id,
                }
            ),
            flush=True,
        )
        deadline = time.monotonic() + args.timeout
        if args.expect_restart:
            while time.monotonic() < deadline:
                try:
                    call(args.binary, args.address, {"op": "get", "id": decision_id})
                except (subprocess.CalledProcessError, subprocess.TimeoutExpired):
                    print(
                        json.dumps({"stage": "unavailable", "host": host}), flush=True
                    )
                    break
                time.sleep(0.5)
            else:
                raise TimeoutError("service interruption not observed")
            while time.monotonic() < deadline:
                try:
                    decision_retry = call(args.binary, args.address, decision_operation)
                    request_retry = call(args.binary, args.address, request_operation)
                except (subprocess.CalledProcessError, subprocess.TimeoutExpired):
                    time.sleep(0.5)
                    continue
                if (
                    decision_retry["id"] != decision_id
                    or request_retry["id"] != request_id
                ):
                    raise RuntimeError("retries returned different IDs after recovery")
                print(
                    json.dumps(
                        {
                            "stage": "recovered",
                            "host": host,
                            "request_id": request_id,
                            "decision_id": decision_id,
                        }
                    ),
                    flush=True,
                )
                break
            else:
                raise TimeoutError("service did not recover")
        while time.monotonic() < deadline:
            response = call(
                args.binary,
                args.address,
                {"op": "get_message_response", "id": request_id},
            )["response"]
            if response is not None:
                message = response["message"]
                if (
                    message["sender"] != peer
                    or message["body"] != reply_body
                    or message["activity"] != channel
                ):
                    raise RuntimeError("unexpected response")
                answer = call(
                    args.binary, args.address, {"op": "get", "id": decision_id}
                )["answer"]
                if answer != {"type": "suggested", "index": 1, "value": "Yes"}:
                    raise RuntimeError("unexpected decision answer")
                print(
                    json.dumps(
                        {
                            "stage": "complete",
                            "host": host,
                            "request_id": request_id,
                            "reply_id": response["id"],
                            "decision_id": decision_id,
                        }
                    ),
                    flush=True,
                )
                return
            time.sleep(0.5)
        raise TimeoutError("peer response not observed")

    # The peer starts after the origin reports ready, so its first read recovers
    # a message accepted before this client was running.
    page = call(
        args.binary,
        args.address,
        {"op": "inbox_page", "participant": peer, "offset": 0},
    )
    matches = [
        item for item in page["messages"] if item["message"]["identity"] == identity
    ]
    if len(matches) != 1 or page["next_offset"] is not None:
        raise RuntimeError("expected exactly one request in the fresh test inbox")
    item = matches[0]
    message = item["message"]
    prefix = f"request {token} decision "
    if (
        message["sender"] != origin
        or not message["body"].startswith(prefix)
        or not message["expects_reply"]
        or message["activity"] != channel
    ):
        raise RuntimeError("unexpected request")
    decision_id = int(message["body"][len(prefix) :])
    history = call(
        args.binary,
        args.address,
        {
            "op": "history_page",
            "kind": "decision",
            "id": decision_id,
            "section": "decisions",
        },
    )
    decision = history["decisions"][0]
    if (
        decision["request"]["identity"] != f"decision-{token}"
        or decision["answer"] is not None
    ):
        raise RuntimeError("unexpected decision")
    call(args.binary, args.address, {"op": "answer", "id": decision_id, "index": 1})
    operation = {
        "op": "reply_message",
        "reply": {
            "identity": f"reply-{token}",
            "request_id": item["id"],
            "sender": peer,
            "body": reply_body,
        },
    }
    reply = call(args.binary, args.address, operation)
    if call(args.binary, args.address, operation) != reply:
        raise RuntimeError("identical reply retry did not return the same result")
    print(
        json.dumps(
            {
                "stage": "complete",
                "host": host,
                "request_id": item["id"],
                "reply_id": reply["id"],
                "decision_id": decision_id,
            }
        ),
        flush=True,
    )


if __name__ == "__main__":
    main()
