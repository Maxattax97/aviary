use crate::backend::GenericSocketBackend;
use crate::codec::*;
use crate::endpoint::Endpoint;
use crate::error::{ZmqError, ZmqResult};
use crate::fair_queue::FairQueue;
use crate::message::*;
use crate::transport::AcceptStopHandle;
use crate::util::PeerIdentity;
use crate::{MultiPeerBackend, SocketEvent, SocketOptions, SocketRecv, SocketSend, SocketType};
use crate::{Socket, SocketBackend};

use async_trait::async_trait;
use futures::channel::mpsc;
use futures::StreamExt;

use std::collections::HashMap;
use std::convert::TryInto;
use std::sync::Arc;

pub struct RouterSocket {
    backend: Arc<GenericSocketBackend>,
    binds: HashMap<Endpoint, AcceptStopHandle>,
    fair_queue: FairQueue<ZmqFramedRead, PeerIdentity>,
}

impl Drop for RouterSocket {
    fn drop(&mut self) {
        self.backend.shutdown();
    }
}

#[async_trait]
impl Socket for RouterSocket {
    fn with_options(options: SocketOptions) -> Self {
        let mut fair_queue = FairQueue::new(true);
        let backend = Arc::new(GenericSocketBackend::with_options(
            Some(fair_queue.inner()),
            SocketType::ROUTER,
            options,
        ));

        let backend_weak = Arc::downgrade(&backend);
        fair_queue.set_on_disconnect(move |peer_id: PeerIdentity| {
            if let Some(backend) = backend_weak.upgrade() {
                backend.peer_disconnected(&peer_id);
            }
        });

        Self {
            backend,
            binds: HashMap::new(),
            fair_queue,
        }
    }

    fn backend(&self) -> Arc<dyn MultiPeerBackend> {
        self.backend.clone()
    }

    fn binds(&mut self) -> &mut HashMap<Endpoint, AcceptStopHandle> {
        &mut self.binds
    }

    fn monitor(&mut self) -> mpsc::Receiver<SocketEvent> {
        let (sender, receiver) = mpsc::channel(1024);
        self.backend.socket_monitor.lock().replace(sender);
        receiver
    }
}

#[async_trait]
impl SocketRecv for RouterSocket {
    async fn recv(&mut self) -> ZmqResult<ZmqMessage> {
        loop {
            match self.fair_queue.next().await {
                Some((peer_id, Ok(Message::Message(mut message)))) => {
                    message.push_front(peer_id.into());
                    return Ok(message);
                }
                Some((_peer_id, Ok(_msg))) => {
                    // todo: Log or handle other message types if needed
                    // We could take an approach of using `tracing` and have that be an optional feature
                    // tracing::warn!("Received unimplemented message type: {:?}", msg);
                }
                Some((peer_id, Err(_e))) => {
                    self.backend.peer_disconnected(&peer_id);
                    // We could take an approach of using `tracing` and have that be an optional feature
                    // tracing::error!("Error receiving message from peer {}: {:?}", peer_id, e);
                }
                None => {
                    // The fair queue is empty, which shouldn't happen in normal operation
                    return Err(ZmqError::NoMessage);
                }
            };
        }
    }
}

#[async_trait]
impl SocketSend for RouterSocket {
    async fn send(&mut self, mut message: ZmqMessage) -> ZmqResult<()> {
        if message.len() <= 1 {
            return Err(ZmqError::Socket(
                "ROUTER send requires at least 2 frames: identity frame + message",
            ));
        }
        let peer_id: PeerIdentity = message.pop_front().unwrap().try_into()?;
        self.backend
            .send_to_peer(&peer_id, Message::Message(message))
            .await
    }
}

impl RouterSocket {
    /// Return whether a routing identity currently has a connected peer.
    pub fn has_peer(&self, identity: &[u8]) -> bool {
        PeerIdentity::try_from(identity).is_ok_and(|peer| self.backend.peers.contains_sync(&peer))
    }

    /// Queue a ROUTER reply without waiting for a slow peer.
    pub async fn try_send(&mut self, mut message: ZmqMessage) -> ZmqResult<()> {
        if message.len() <= 1 {
            return Err(ZmqError::Socket(
                "ROUTER send requires an identity and a payload",
            ));
        }
        let identity: PeerIdentity = message
            .pop_front()
            .ok_or(ZmqError::PeerIdentity)?
            .try_into()?;
        let Some(mut peer) = self.backend.peers.get_async(&identity).await else {
            return Err(ZmqError::Other("Destination client not found by identity"));
        };
        match peer.send_queue.try_send(Message::Message(message)) {
            Ok(()) => Ok(()),
            Err(error) if error.is_full() => Err(ZmqError::BufferFull("peer send queue full")),
            Err(_) => Err(ZmqError::Other("peer send queue disconnected")),
        }
    }

    /// Splits the socket into separate send and recv halves, allowing concurrent
    /// sending and receiving from independent async tasks.
    ///
    /// The underlying socket stays alive until both halves are dropped.
    pub fn split(mut self) -> (RouterSendHalf, RouterRecvHalf) {
        let backend = std::mem::replace(
            &mut self.backend,
            Arc::new(GenericSocketBackend::with_options(
                None,
                SocketType::ROUTER,
                SocketOptions::default(),
            )),
        );
        let fair_queue = std::mem::replace(&mut self.fair_queue, FairQueue::new(true));
        let binds = std::mem::take(&mut self.binds);

        let inner = Arc::new(RouterSocketInner {
            backend,
            _binds: binds,
        });

        (
            RouterSendHalf {
                inner: inner.clone(),
            },
            RouterRecvHalf { inner, fair_queue },
        )
    }
}

#[cfg(test)]
mod aviary_backpressure_tests {
    use super::*;
    use crate::backend::{Peer, PEER_SEND_QUEUE_CAPACITY};
    use bytes::Bytes;
    use futures::StreamExt;

    fn routed(peer: &str, payload: &'static [u8]) -> ZmqMessage {
        let mut message = ZmqMessage::from(Bytes::copy_from_slice(peer.as_bytes()));
        message.push_back(Bytes::from_static(payload));
        message
    }

    #[crate::async_rt::test]
    async fn full_subscriber_queue_keeps_hints_and_does_not_block_another_peer() {
        let mut router = RouterSocket::with_options(SocketOptions::default());
        let (slow_sender, mut slow_receiver) = mpsc::channel(PEER_SEND_QUEUE_CAPACITY);
        let (fast_sender, mut fast_receiver) = mpsc::channel(PEER_SEND_QUEUE_CAPACITY);
        let slow: PeerIdentity = "slow-subscriber".parse().expect("slow peer identity");
        let fast: PeerIdentity = "fast-subscriber".parse().expect("fast peer identity");
        router
            .backend
            .peers
            .upsert_async(
                slow,
                Peer {
                    send_queue: slow_sender,
                },
            )
            .await;
        router
            .backend
            .peers
            .upsert_async(
                fast,
                Peer {
                    send_queue: fast_sender,
                },
            )
            .await;

        let mut queued = 0;
        loop {
            match router.try_send(routed("slow-subscriber", b"hint")).await {
                Ok(()) => queued += 1,
                Err(ZmqError::BufferFull(_)) => break,
                Err(error) => panic!("unexpected queue error: {error}"),
            }
            assert!(
                queued <= PEER_SEND_QUEUE_CAPACITY + 2,
                "peer queue was not bounded"
            );
        }
        assert!(queued > 0);
        router
            .try_send(routed("fast-subscriber", b"hint"))
            .await
            .expect("other peer remains writable");
        assert!(matches!(
            fast_receiver.next().await,
            Some(Message::Message(_))
        ));
        for _ in 0..queued {
            let Some(Message::Message(message)) = slow_receiver.next().await else {
                panic!("queued hint missing");
            };
            let frames = message.into_vec();
            assert_eq!(frames.len(), 1);
            assert_eq!(frames.first().map(Bytes::as_ref), Some(&b"hint"[..]));
        }
        router
            .try_send(routed("slow-subscriber", b"later"))
            .await
            .expect("drained queue accepts a later hint");
    }
}

struct RouterSocketInner {
    backend: Arc<GenericSocketBackend>,
    _binds: HashMap<Endpoint, AcceptStopHandle>,
}

impl Drop for RouterSocketInner {
    fn drop(&mut self) {
        self.backend.shutdown();
    }
}

/// The send half of a [`RouterSocket`] produced by [`RouterSocket::split`].
///
/// This half can be cloned to send from multiple async tasks concurrently.
#[derive(Clone)]
pub struct RouterSendHalf {
    inner: Arc<RouterSocketInner>,
}

/// The recv half of a [`RouterSocket`] produced by [`RouterSocket::split`].
pub struct RouterRecvHalf {
    inner: Arc<RouterSocketInner>,
    fair_queue: FairQueue<ZmqFramedRead, PeerIdentity>,
}

#[async_trait]
impl SocketSend for RouterSendHalf {
    async fn send(&mut self, mut message: ZmqMessage) -> ZmqResult<()> {
        if message.len() <= 1 {
            return Err(ZmqError::Socket(
                "ROUTER send requires at least 2 frames: identity frame + message",
            ));
        }
        let peer_id: PeerIdentity = message.pop_front().unwrap().try_into()?;
        self.inner
            .backend
            .send_to_peer(&peer_id, Message::Message(message))
            .await
    }
}

#[async_trait]
impl SocketRecv for RouterRecvHalf {
    async fn recv(&mut self) -> ZmqResult<ZmqMessage> {
        loop {
            match self.fair_queue.next().await {
                Some((peer_id, Ok(Message::Message(mut message)))) => {
                    message.push_front(peer_id.into());
                    return Ok(message);
                }
                Some((_peer_id, Ok(_))) => {}
                Some((peer_id, Err(_e))) => {
                    self.inner.backend.peer_disconnected(&peer_id);
                }
                None => {
                    return Err(ZmqError::NoMessage);
                }
            };
        }
    }
}
