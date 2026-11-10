#!/usr/bin/env python3
"""Execute the portable skill's shell examples against a local service."""

import json
import os
import re
import subprocess

from support import BINARY, ROOT, Service


def main() -> None:
    skill = (ROOT / "skills" / "aviary" / "SKILL.md").read_text()
    blocks = re.findall(r"```sh\n(.*?)\n```", skill, re.DOTALL)
    if len(blocks) != 2:
        raise AssertionError(f"Expected two executable examples, found {len(blocks)}")

    with Service() as service:
        service.call(
            {
                "op": "create_channel",
                "channel": {
                    "name": "team",
                    "members": ["agent-a", "agent-b"],
                },
            }
        )
        env = dict(
            os.environ,
            PATH=f"{BINARY.parent}:{os.environ.get('PATH', '')}",
            AVIARY_ADDR=service.address,
            AVIARY_AGENT="agent-a",
            AVIARY_CHANNEL="team",
            AVIARY_RECIPIENT="agent-b",
        )

        def run_example(index: int, identity: str) -> list[dict]:
            env["AVIARY_KEY"] = identity
            result = subprocess.run(
                ["/bin/sh", "-eu", "-c", blocks[index]],
                env=env,
                text=True,
                capture_output=True,
                timeout=15,
                check=True,
            )
            return [json.loads(line) for line in result.stdout.splitlines()]

        decision = run_example(0, "skill-decision")
        assert len(decision) == 2
        assert decision[0]["id"] == decision[1]["id"]
        assert decision[1]["answer"] is None

        message = run_example(1, "skill-message")
        assert message[0]["id"] == message[1]["messages"][0]["id"]
        assert message[1]["messages"][0]["message"]["body"] == "Please review"
        failure = service.run({"op": "get", "id": 999999})
        assert failure.returncode != 0 and "request not found" in failure.stderr
    print("Skill examples: decision, message, retrieval and error status passed")


if __name__ == "__main__":
    main()
