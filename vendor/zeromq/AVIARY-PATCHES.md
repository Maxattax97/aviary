# Aviary local modifications to zeromq 0.7.0

This directory contains the MIT-licensed upstream Rust ZeroMQ implementation.
Aviary uses a local copy because upstream 0.7.0 does not expose receive-frame,
aggregate-message, multipart-frame-count, or peer-send-queue limits.

- `SocketOptions::message_limits` sets receive bounds before allocating a
  frame. An oversized frame closes its peer connection. An overlong multipart
  message is discarded one frame at a time, after which the connection can
  receive the next message.
- The peer send queue is limited to 64 messages. Aviary bounds a reply enqueue
  with a timeout and treats a lost reply as an unknown outcome.
- Unit tests cover oversized frame headers and multipart recovery.

These are Aviary-specific changes, not claims about upstream support. Review
them against upstream changes when updating this copy. The license is in
`LICENSE.md`.
