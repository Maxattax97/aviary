#!/usr/bin/env python3
"""Populate a running Aviary service with sample operator work."""

import argparse
import json
import subprocess
import sys
import uuid
from pathlib import Path

BINARY = Path(__file__).resolve().parent.parent / "target" / "debug" / "aviary"


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Add persistent sample decisions, messages, and notices to a running Aviary service."
    )
    parser.add_argument("address", nargs="?", default="127.0.0.1:7432", help="service address (default: 127.0.0.1:7432)")
    args = parser.parse_args()
    if not BINARY.is_file():
        parser.error("Aviary is not built; run cargo build --locked first")

    prefix = "demo-" + uuid.uuid4().hex
    channel = prefix + "-team"

    def call(operation: dict) -> None:
        subprocess.run(
            [str(BINARY), "call", args.address, json.dumps(operation)],
            check=True,
            stdout=subprocess.DEVNULL,
        )

    agents = ["planner", "builder", "reviewer"]
    call({"op": "create_channel", "channel": {"name": channel, "members": ["operator", *agents]}})
    decisions = [
        ("planner", "Which feature should we tackle first?", ["Search", "Notifications", "Export"], 0, "Search unblocks the most workflows."),
        ("builder", "May I update the dependency lockfile?", ["Approve", "Decline"], 0, "The test suite passes with the update."),
        ("reviewer", "How should we handle the failing integration test?", ["Investigate now", "Defer"], 0, "It may indicate a regression."),
        ("planner", "What should the next milestone be called?", [], None, None),
        ("builder", "Should the preview include draft content?", ["Include drafts", "Published only"], None, None),
    ]
    for index, (agent, question, options, recommendation, reason) in enumerate(decisions):
        request = {"identity": f"{prefix}-decision-{index}", "agent": agent, "question": question,
                   "options": options, "importance": len(decisions) - index,
                   "explanation": "Sample item for exploring the decision queue.",
                   "context": "This is simulated work. No agent is waiting for an answer.", "activity": prefix}
        if recommendation is not None:
            request.update(recommendation=recommendation, reason=reason)
        call({"op": "submit", "request": request})

    messages = [
        ("planner", "I have drafted a plan for the next milestone.", False),
        ("builder", "The prototype is ready for an initial look.", False),
        ("reviewer", "Could you clarify the expected behavior for empty results?", True),
        ("builder", "The test environment is available when you are ready.", False),
        ("planner", "I have collected the open questions in one place.", False),
    ]
    for index, (agent, body, expects_reply) in enumerate(messages):
        call({"op": "send_message", "message": {"identity": f"{prefix}-message-{index}",
              "channel": channel, "sender": agent, "target": {"kind": "participant", "name": "operator"},
              "body": body, "expects_reply": expects_reply,
              "context": "Simulated project work", "activity": prefix}})

    for index, (agent, work, result) in enumerate([
        ("builder", "Build prototype", "Build completed successfully."),
        ("reviewer", "Review changes", "Review completed; two suggestions recorded."),
        ("planner", "Prepare milestone", "Milestone outline is ready."),
    ]):
        call({"op": "submit_notice", "notice": {"identity": f"{prefix}-notice-{index}",
              "agent": agent, "work": work, "result": result, "activity": prefix}})

    print(f"Added 5 decisions, 5 operator messages, and 3 notices to {args.address}.")
    print(f"Sample activity: {prefix}; sample channel: {channel}")
    print("Open the terminal with: ./target/debug/aviary tui " + args.address)
    print("Use h/l to browse decisions, i for messages, n for notices, and ? for help.")
    print("These sample records persist in the service database; no agents are actually waiting.")


if __name__ == "__main__":
    try:
        main()
    except subprocess.CalledProcessError as error:
        sys.exit(f"Seeding stopped: an Aviary call failed (exit {error.returncode}); earlier sample records may remain.")
