#!/usr/bin/env python3
"""Exercise bounded stdin operations, including an argument-limit-sized payload."""

import json
import subprocess

from support import BINARY, Service


def main() -> None:
    with Service() as service:
        operation = {
            "op": "submit",
            "request": {
                "identity": "stdin-large",
                "agent": "agent",
                "question": "Review large context",
                "options": [],
                "context": "x" * 1_047_000,
            },
        }
        encoded = json.dumps(operation)
        assert 131_072 < len(encoded.encode()) < 1_048_576

        def run(data: str) -> subprocess.CompletedProcess[str]:
            return subprocess.run(
                [str(BINARY), "call", service.address, "-"],
                input=data,
                text=True,
                capture_output=True,
                timeout=15,
            )

        first = run(encoded)
        assert first.returncode == 0, first.stderr
        request_id = json.loads(first.stdout)["id"]
        retry = run(encoded)
        assert retry.returncode == 0 and json.loads(retry.stdout)["id"] == request_id
        page = service.call({"op": "list_page"})["requests"]
        assert len(page) == 1 and page[0]["id"] == request_id
        assert page[0]["request"]["context"] == operation["request"]["context"]
        malformed = run('{"op":')
        assert malformed.returncode != 0 and malformed.stdout == ""
        oversized = run(" " * 1_048_577)
        assert oversized.returncode != 0 and oversized.stdout == ""
        assert "message too large or incomplete" in oversized.stderr
        assert len(service.call({"op": "list_page"})["requests"]) == 1
    print("CLI stdin: near-limit submission, retry and bounded errors passed")


if __name__ == "__main__":
    main()
