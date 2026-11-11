#!/usr/bin/env python3
"""Keep a custom draft tied to its original decision while navigating."""
from support import Service, Terminal


def submit(service, identity, question):
    return service.call({"op": "submit", "request": {"identity": identity, "agent": "agent",
        "question": question, "context": "Detail context", "options": []}})["id"]


def main():
    with Service() as service:
        first = submit(service, "draft-original", "Original decision")
        second = submit(service, "draft-other", "Other decision")
        with Terminal(service) as terminal:
            terminal.expect("Original decision")
            terminal.send("j\rexact 鳥 draft")
            terminal.expect("exact 鳥 draft")
            terminal.send("\x07")  # Ctrl-G help while editing
            terminal.expect("Keyboard help")
            terminal.send("\x1b")
            terminal.expect("exact 鳥 draft")
            terminal.send("\x0f")  # Ctrl-O suspend
            terminal.expect("Answer draft suspended")
            terminal.send("l")
            terminal.expect("Other decision")
            terminal.send("j\r")
            terminal.expect("Answer draft held; Ctrl-O resumes it")
            terminal.send("c")
            terminal.expect("Detail context")
            terminal.send("\x0f")
            terminal.expect("exact 鳥 draft")
            terminal.send("\x0f")
            terminal.expect("Answer draft suspended")
            terminal.send("i")
            terminal.expect("Operator inbox")
            terminal.send("\x0f")
            terminal.expect("exact 鳥 draft")
            terminal.expect("Original decision")
            assert service.call({"op": "get", "id": first})["answer"] is None
            assert service.call({"op": "get", "id": second})["answer"] is None
            terminal.send("\r")
            service.wait_for(lambda: service.call({"op": "get", "id": first})["answer"] ==
                {"type": "custom", "value": "exact 鳥 draft"}, "original custom answer")
            assert service.call({"op": "get", "id": second})["answer"] is None
            terminal.expect("Other decision")
            terminal.quit()
        gone = submit(service, "draft-gone", "Decision removed")
        with Terminal(service) as terminal:
            terminal.expect("Other decision")
            terminal.send("l")
            terminal.expect("Decision removed")
            terminal.send("j\rheld text")
            terminal.expect("held text")
            terminal.send("\x0f")
            terminal.expect("Answer draft suspended")
            service.call({"op": "answer_custom", "id": gone, "value": "already resolved"})
            terminal.send("\x0f")
            terminal.expect("held text")
            terminal.expect("Request not on this queue page; draft retained")
            terminal.send("\r")
            terminal.expect("Rejected:")
            assert service.call({"op": "get", "id": gone})["answer"]["value"] == "already resolved"
            assert service.call({"op": "get", "id": second})["answer"] is None
            terminal.send("\x1b")
            terminal.expect("Other decision")
            terminal.quit()
    print("Custom draft survived navigation and retained its original decision after removal")


if __name__ == "__main__":
    main()
