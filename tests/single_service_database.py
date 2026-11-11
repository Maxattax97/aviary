#!/usr/bin/env python3
"""Reject two services on one database and recover after the owner exits."""
import json
import os
import socket
import subprocess
from pathlib import Path

from support import BINARY, Service
from terminal_reconnect import listening, stop


def main():
    with Service() as service:
        database = Path(service.directory.name) / "state.sqlite"
        request = {"op": "submit", "request": {"identity": "single-owner", "agent": "agent", "question": "Retained", "options": []}}
        initial = service.call(request)
        with socket.socket() as probe:
            probe.bind(("127.0.0.1", 0))
            port = probe.getsockname()[1]
        address = f"127.0.0.1:{port}"
        symlink = database.with_name("alias.sqlite")
        symlink.symlink_to(database)
        hardlink = database.with_name("hardlink.sqlite")
        os.link(database, hardlink)
        for candidate in (database, symlink, hardlink):
            conflict = subprocess.run([str(BINARY), "serve", str(candidate), address], text=True, capture_output=True, timeout=10)
            assert conflict.returncode != 0 and "database is already in use" in conflict.stderr, conflict.stderr
            assert service.call(request) == initial
            assert not listening("127.0.0.1", port)
        with socket.socket() as probe:
            probe.bind(("127.0.0.1", 0))
            independent_port = probe.getsockname()[1]
        independent = subprocess.Popen([str(BINARY), "serve", str(database.with_name("independent.sqlite")), f"127.0.0.1:{independent_port}"], stderr=subprocess.DEVNULL)
        try:
            service.wait_for(lambda: independent.poll() is not None or listening("127.0.0.1", independent_port), "independent service")
            assert independent.poll() is None
            assert service.call(request) == initial
        finally:
            if independent.poll() is None:
                independent.terminate()
                independent.wait(timeout=8)
        stop(service)
        replacement = subprocess.Popen([str(BINARY), "serve", str(database), address], stderr=subprocess.DEVNULL)
        try:
            service.wait_for(lambda: replacement.poll() is not None or listening("127.0.0.1", port), "replacement service")
            assert replacement.poll() is None
            result = subprocess.run([str(BINARY), "call", address, json.dumps(request)], capture_output=True, text=True, timeout=10, check=True)
            assert json.loads(result.stdout) == initial
        finally:
            if replacement.poll() is None:
                replacement.terminate()
                replacement.wait(timeout=8)
    print("Duplicate database owners rejected, including aliases; replacement recovered retained work")


if __name__ == "__main__":
    main()
