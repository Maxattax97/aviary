#!/usr/bin/env python3
"""Drop one committed instruction response and exercise the terminal retry."""
import ctypes
import json
import os
import select
import socket
import threading
import time
from types import SimpleNamespace

from support import Service, Terminal
from zeromq_interop import Dealer, ZMQ_LINGER, ZMQ_RCVTIMEO


def main() -> None:
    with Service() as service:
        service.call({"op": "create_channel", "channel": {"name": "all", "members": ["operator", "worker"]}})
        with socket.socket() as port:
            port.bind(("127.0.0.1", 0))
            address = f"127.0.0.1:{port.getsockname()[1]}"
        ready = threading.Event()
        stopped = threading.Event()
        dropped = threading.Event()
        calls = []
        errors = []

        def proxy() -> None:
            dealer = Dealer(service.address)
            lib = dealer.lib
            lib.zmq_bind.argtypes = [ctypes.c_void_p, ctypes.c_char_p]
            router = lib.zmq_socket(dealer.context, 6)  # ROUTER
            try:
                for option, value in [(ZMQ_LINGER, 0), (ZMQ_RCVTIMEO, 100)]:
                    number = ctypes.c_int(value)
                    dealer.check(lib.zmq_setsockopt(router, option, ctypes.byref(number), ctypes.sizeof(number)))
                dealer.check(lib.zmq_bind(router, f"tcp://{address}".encode()))
                ready.set()
                while not stopped.is_set():
                    route = ctypes.create_string_buffer(256)
                    size = lib.zmq_recv(router, route, len(route), 0)
                    if size < 0:
                        continue
                    payload = ctypes.create_string_buffer(1_048_577)
                    length = dealer.check(lib.zmq_recv(router, payload, len(payload), 0))
                    request = json.loads(payload.raw[:length])
                    operation = request["operation"]
                    dealer.send(request["correlation"], operation, version=2)
                    response = dealer.receive(version=2)
                    if operation["op"] == "send_message":
                        calls.append(operation["message"])
                        if not dropped.is_set():
                            dropped.set()
                            continue
                    raw = json.dumps(response).encode()
                    dealer.check(lib.zmq_send(router, route, size, 2))  # SNDMORE
                    dealer.check(lib.zmq_send(router, raw, len(raw), 0))
            except BaseException as error:
                errors.append(error)
                ready.set()
            finally:
                lib.zmq_close(router)
                dealer.close()

        thread = threading.Thread(target=proxy, daemon=True)
        thread.start()
        assert ready.wait(5) and not errors, errors
        try:
            with Terminal(SimpleNamespace(address=address)) as terminal:
                terminal.expect("No active requests")
                terminal.send("m")
                terminal.expect("Compose instruction")
                terminal.send("Commit once\r")
                terminal.expect("Destination: all")
                terminal.send("\x13")
                terminal.expect("Action in progress")
                assert dropped.wait(5)
                terminal.send("\x13")  # ignored while the first action is pending
                terminal.expect("Unconfirmed:", timeout=16)
                time.sleep(0.6)
                assert len(calls) == 1
                terminal.send("\x0f")  # suspend without resubmitting
                terminal.expect("Instruction draft suspended")
                terminal.send("m")
                terminal.expect("Unconfirmed:")
                assert len(calls) == 1
                inbox = service.call({"op": "inbox", "participant": "worker"})["messages"]
                assert len(inbox) == 1 and inbox[0]["message"]["body"] == "Commit once"
                terminal.send("\x13")
                terminal.expect("Instruction sent")
                deadline = time.monotonic() + 6
                expired = False
                while time.monotonic() < deadline:
                    if select.select([terminal.master], [], [], 0.2)[0]:
                        terminal._feed_screen(os.read(terminal.master, 65536))
                        screen = "\n".join("".join(row) for row in terminal.screen)
                        if "Instruction sent" not in screen:
                            expired = True
                            break
                assert expired, "success notification did not disappear"
                assert len(calls) == 2 and calls[0] == calls[1]
                assert len(service.call({"op": "inbox", "participant": "worker"})["messages"]) == 1
                terminal.quit()
        finally:
            stopped.set()
            thread.join(5)
        assert not thread.is_alive() and not errors, errors
    print("Lost committed acknowledgment stays unconfirmed; explicit identical retry does not duplicate")


if __name__ == "__main__":
    main()
