#!/usr/bin/env python3
"""Run the CLI reference's executable workflow against an isolated service."""

import json
import os
import re
import subprocess

from support import BINARY, ROOT, Service


def main() -> None:
    reference = (ROOT / "docs" / "cli.md").read_text()
    examples = re.findall(r"```python\n(.*?)\n```", reference, re.DOTALL)
    if len(examples) != 1:
        raise AssertionError(f"Expected one executable example, found {len(examples)}")
    with Service() as service:
        env = dict(
            os.environ,
            AVIARY_ADDR=service.address,
            PATH=f"{BINARY.parent}:{os.environ.get('PATH', '')}",
        )
        result = subprocess.run(
            ["python3", "-c", examples[0]],
            env=env,
            text=True,
            capture_output=True,
            timeout=30,
            check=True,
        )
        output = json.loads(result.stdout)
        assert output["answer"] == {"type": "suggested", "index": 1, "value": "Yes"}
        assert all(
            isinstance(output[field], int)
            for field in ("decision_id", "message_id", "reply_id")
        )
    print("CLI reference workflow: decision, message, reply, notice and history passed")


if __name__ == "__main__":
    main()
