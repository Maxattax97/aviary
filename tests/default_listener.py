#!/usr/bin/env python3
"""Inspect the default service listener on Linux without using another service."""

import json
import os
from pathlib import Path
import socket
import struct
import subprocess
import sys
import tempfile
import time

from support import BINARY

PORT = 7432


def process_listeners(pid: int) -> list[tuple[str, str]]:
    root = Path(f"/proc/{pid}")
    inodes = set()
    for fd in (root / "fd").iterdir():
        try:
            target = os.readlink(fd)
        except FileNotFoundError:
            continue
        if target.startswith("socket:[") and target.endswith("]"):
            inodes.add(target[8:-1])
    listeners = []
    for family in ("tcp", "tcp6"):
        for line in (root / "net" / family).read_text().splitlines()[1:]:
            columns = line.split()
            if columns[3] != "0A" or columns[9] not in inodes:
                continue
            address_hex, port_hex = columns[1].split(":")
            if int(port_hex, 16) != PORT:
                continue
            if family == "tcp":
                address = socket.inet_ntop(
                    socket.AF_INET, struct.pack("=I", int(address_hex, 16))
                )
            else:
                address = address_hex
            listeners.append((family, address))
    return listeners


def main() -> None:
    if sys.platform != "linux":
        print("SKIP: default listener inspection requires Linux procfs")
        return
    if not BINARY.is_file():
        raise RuntimeError("Build Aviary first: cargo build --locked")
    # Never connect to an existing instance. A conflicting listener means this
    # test cannot own the default port; it is a skip, not a passing observation.
    with socket.socket() as probe:
        try:
            probe.bind(("0.0.0.0", PORT))
        except OSError as error:
            print(f"SKIP: default port is unavailable: {error}")
            return
    with tempfile.TemporaryDirectory(prefix="aviary-default-listener-") as directory:
        process = subprocess.Popen(
            [str(BINARY), "serve"],
            cwd=directory,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.PIPE,
        )
        try:
            deadline = time.monotonic() + 8
            while True:
                if process.poll() is not None:
                    raise RuntimeError(
                        "test service exited before its listener was observed"
                    )
                listeners = process_listeners(process.pid)
                if listeners:
                    break
                if time.monotonic() >= deadline:
                    raise TimeoutError("test service listener was not observed")
                time.sleep(0.05)
            assert listeners == [("tcp", "127.0.0.1")], listeners
            result = subprocess.run(
                [str(BINARY), "call", f"127.0.0.1:{PORT}", '{"op":"list_page"}'],
                capture_output=True,
                text=True,
                check=True,
                timeout=15,
            )
            assert json.loads(result.stdout)["requests"] == []
            print(
                "Default service owns only 127.0.0.1:7432 and accepts a local request"
            )
        finally:
            if process.poll() is None:
                process.terminate()
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait(timeout=5)
                raise
            if process.stderr is not None:
                process.stderr.close()


if __name__ == "__main__":
    main()
