#!/usr/bin/env python3
"""Exercise the wire contract through Python's ctypes and system libzmq."""

import ctypes
import ctypes.util
import json

from support import Service
from terminal_reconnect import restart, stop

ZMQ_DEALER = 5
ZMQ_RCVMORE = 13
ZMQ_LINGER = 17
ZMQ_RCVTIMEO = 27
ZMQ_SNDTIMEO = 28
MAX_FRAME = 1_048_576


class Dealer:
    def __init__(self, address: str) -> None:
        name = ctypes.util.find_library("zmq")
        if not name:
            raise RuntimeError(
                "System libzmq is required for the interoperability test"
            )
        self.lib = ctypes.CDLL(name)
        pointer = ctypes.c_void_p
        self.lib.zmq_ctx_new.restype = pointer
        self.lib.zmq_ctx_term.argtypes = [pointer]
        self.lib.zmq_socket.argtypes = [pointer, ctypes.c_int]
        self.lib.zmq_socket.restype = pointer
        self.lib.zmq_close.argtypes = [pointer]
        self.lib.zmq_connect.argtypes = [pointer, ctypes.c_char_p]
        self.lib.zmq_setsockopt.argtypes = [
            pointer,
            ctypes.c_int,
            pointer,
            ctypes.c_size_t,
        ]
        self.lib.zmq_getsockopt.argtypes = [
            pointer,
            ctypes.c_int,
            pointer,
            ctypes.POINTER(ctypes.c_size_t),
        ]
        self.lib.zmq_send.argtypes = [pointer, pointer, ctypes.c_size_t, ctypes.c_int]
        self.lib.zmq_recv.argtypes = [pointer, pointer, ctypes.c_size_t, ctypes.c_int]
        self.lib.zmq_errno.restype = ctypes.c_int
        self.lib.zmq_strerror.argtypes = [ctypes.c_int]
        self.lib.zmq_strerror.restype = ctypes.c_char_p
        self.context = self.lib.zmq_ctx_new()
        if not self.context:
            raise RuntimeError("zmq_ctx_new failed")
        self.socket = None
        try:
            self.socket = self.lib.zmq_socket(self.context, ZMQ_DEALER)
            if not self.socket:
                raise RuntimeError("zmq_socket failed")
            for option, value in [
                (ZMQ_LINGER, 0),
                (ZMQ_RCVTIMEO, 5000),
                (ZMQ_SNDTIMEO, 5000),
            ]:
                number = ctypes.c_int(value)
                self.check(
                    self.lib.zmq_setsockopt(
                        self.socket, option, ctypes.byref(number), ctypes.sizeof(number)
                    )
                )
            self.check(self.lib.zmq_connect(self.socket, f"tcp://{address}".encode()))
        except BaseException:
            self.close()
            raise

    def check(self, result: int) -> int:
        if result < 0:
            errno = self.lib.zmq_errno()
            raise RuntimeError(f"libzmq: {self.lib.zmq_strerror(errno).decode()}")
        return result

    def send(self, correlation: str, operation: dict, version: int = 1) -> None:
        payload = json.dumps(
            {"version": version, "correlation": correlation, "operation": operation},
            ensure_ascii=False,
        ).encode()
        buffer = ctypes.create_string_buffer(payload)
        assert self.check(
            self.lib.zmq_send(self.socket, buffer, len(payload), 0)
        ) == len(payload)

    def receive(self) -> dict:
        buffer = ctypes.create_string_buffer(MAX_FRAME + 1)
        size = self.check(self.lib.zmq_recv(self.socket, buffer, len(buffer), 0))
        if size > MAX_FRAME:
            raise AssertionError("oversized response")
        more = ctypes.c_int()
        option_size = ctypes.c_size_t(ctypes.sizeof(more))
        self.check(
            self.lib.zmq_getsockopt(
                self.socket, ZMQ_RCVMORE, ctypes.byref(more), ctypes.byref(option_size)
            )
        )
        assert more.value == 0
        response = json.loads(buffer.raw[:size])
        assert response["version"] == 1
        return response

    def close(self) -> None:
        if self.socket:
            self.lib.zmq_close(self.socket)
            self.socket = None
        if self.context:
            self.lib.zmq_ctx_term(self.context)
            self.context = None


def main() -> None:
    with Service() as service:
        dealer = Dealer(service.address)
        try:
            request = {
                "identity": "ctypes-decision",
                "agent": "python-agent",
                "question": "Proceed 鳥?",
                "options": ["No", "Yes"],
                "recommendation": 1,
                "reason": "Ready",
            }
            dealer.send("submit-a", {"op": "submit", "request": request})
            dealer.send("list-b", {"op": "list_page"})
            replies = {
                reply["correlation"]: reply["result"]
                for reply in (dealer.receive(), dealer.receive())
            }
            assert set(replies) == {"submit-a", "list-b"}
            request_id = replies["submit-a"]["id"]
            assert replies["list-b"]["requests"][0]["id"] == request_id
            assert (
                service.call({"op": "list_page"})["requests"][0]["request"]["question"]
                == "Proceed 鳥?"
            )
            dealer.send("bad-version", {"op": "get", "id": request_id}, version=2)
            assert (
                dealer.receive()["result"]["error"] == "message too large or incomplete"
            )
            stop(service)
            restart(service)
            dealer.send("retry-after-restart", {"op": "submit", "request": request})
            retry = dealer.receive()
            assert (
                retry["correlation"] == "retry-after-restart"
                and retry["result"]["id"] == request_id
            )
            changed = dict(request, question="Changed")
            dealer.send("conflict", {"op": "submit", "request": changed})
            assert dealer.receive()["result"]["error"] == "identity conflict"
            service.call({"op": "answer", "id": request_id, "index": 1})
            dealer.send("answer", {"op": "get", "id": request_id})
            reply = dealer.receive()
            assert reply["correlation"] == "answer"
            assert reply["result"]["answer"] == {
                "type": "suggested",
                "index": 1,
                "value": "Yes",
            }
        finally:
            dealer.close()
    print(
        "Python ctypes/libzmq: pipelining, correlation, restart, retry and answer passed"
    )


if __name__ == "__main__":
    main()
