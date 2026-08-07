//! A generic publish/subscribe [`Broker`] with buffered channels.
//!
//! Mirrors the reference `pubsub.Broker` semantics: any number of
//! subscribers register a buffered channel, `publish` fans a value out to
//! every live subscriber, slow consumers are dropped (the publisher never
//! blocks), and dead subscriptions are removed. Cleanup is context-based via
//! RAII: a [`Subscription`] deregisters itself on drop, so tying a
//! subscription to a request/session/loop scope releases it automatically
//! when that context ends.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, TryRecvError};
use std::sync::{Arc, Mutex, Weak};

struct BrokerInner<T> {
    subscribers: SyncMap<T>,
}

#[allow(non_camel_case_types)]
type SyncMap<T> = std::collections::HashMap<u64, mpsc::SyncSender<T>>;

/// Contract for components that *publish* events onto a bus (implemented by
/// [`Broker`]).
///
/// Mirroring the reference `Publisher` interface: given an event, every live
/// subscriber receives it (best effort — slow consumers are skipped).
pub trait Publisher<T> {
    /// Distribute `event` to all active subscribers, returning how many got it.
    fn publish(&self, event: T) -> usize;
}

impl<T> Publisher<T> for Broker<T>
where
    T: Clone + Send + 'static,
{
    fn publish(&self, event: T) -> usize {
        Broker::publish(self, event)
    }
}

/// A thread-safe, generic event broker hub.
///
/// Values are fanned out by cloning, so the payload type must implement
/// [`Clone`]; it also needs [`Send`] so subscriptions can be moved between
/// threads.
pub struct Broker<T> {
    inner: Arc<Mutex<BrokerInner<T>>>,
    next_id: AtomicU64,
    closed: AtomicBool,
}

impl<T> Broker<T>
where
    T: Clone + Send + 'static,
{
    /// Create a new empty broker.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(BrokerInner {
                subscribers: HashMap::new(),
            })),
            next_id: AtomicU64::new(1),
            closed: AtomicBool::new(false),
        }
    }

    /// Register a subscriber that buffers up to `capacity` events. When the
    /// buffer is full, further events for this subscriber are skipped (slow
    /// consumer) until it drains; it is never allowed to block the publisher.
    ///
    /// The [`Subscription`] deregisters itself automatically when dropped.
    /// If the broker has been [`shutdown`](Broker::shutdown), the returned
    /// subscription is immediately disconnected.
    pub fn subscribe(&self, capacity: usize) -> Subscription<T> {
        let (tx, rx) = mpsc::sync_channel(capacity.max(1));
        if self.closed.load(Ordering::SeqCst) {
            // Refuse new subscribers after shutdown: deliver a disconnected
            // channel so consumers observe zero events and drains immediately.
            return Subscription {
                id: 0,
                rx,
                broker: Weak::new(),
            };
        }
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        self.inner
            .lock()
            .unwrap()
            .subscribers
            .insert(id, tx);
        Subscription {
            id,
            rx,
            broker: Arc::downgrade(&self.inner),
        }
    }

    /// Publish an event to every live subscriber, returning how many received
    /// it. Subscribers with a full buffer simply miss this event (the
    /// publisher never blocks); dropped subscriptions are already deregistered
    /// by RAII and are filtered out as they drain.
    pub fn publish(&self, event: T) -> usize {
        let mut inner = self.inner.lock().unwrap();
        let mut delivered = 0usize;
        inner.subscribers.retain(|_id, tx| match tx.try_send(event.clone()) {
            Ok(()) => {
                delivered += 1;
                true
            }
            Err(_) => true, // buffer full → slow consumer, skip this event
        });
        delivered
    }

    /// Number of currently registered subscriptions.
    pub fn subscriber_count(&self) -> usize {
        self.inner.lock().unwrap().subscribers.len()
    }

    /// Deregister a subscription by id (also performed automatically on drop).
    pub fn remove(&self, id: u64) {
        self.inner.lock().unwrap().subscribers.remove(&id);
    }

    /// Gracefully shut the broker down: the hub stops accepting new
    /// subscriptions and every live subscriber channel is closed, releasing
    /// their buffers (a `recv` immediately reports disconnected).
    ///
    /// This is irreversible — a shut-down broker remains closed.
    pub fn shutdown(&self) {
        self.closed.store(true, Ordering::SeqCst);
        let mut inner = self.inner.lock().unwrap();
        inner.subscribers.clear();
    }

    /// True once [`shutdown`](Broker::shutdown) has been called.
    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }
}

impl<T> Default for Broker<T>
where
    T: Clone + Send + 'static,
{
    fn default() -> Self {
        Self::new()
    }
}

/// Contract for a component that *receives* published events (implemented by
/// [`Subscription`]).
///
/// Mirroring the reference `Subscriber` interface: consumers pull events off
/// their buffered channel — non-blocking with [`try_recv`](Subscriber::try_recv),
/// or blocked until the next event / disconnect with
/// [`recv`](Subscriber::recv).
pub trait Subscriber<T> {
    /// Receive the next buffered event without blocking.
    fn try_recv(&self) -> Result<T, TryRecvError>;

    /// Block until an event arrives; errors once disconnected.
    fn recv(&self) -> Result<T, mpsc::RecvError>;
}

/// A single subscriber's handle on a [`Broker`], backed by a buffered channel.
///
/// Deregisters itself from the broker when dropped (context-based cleanup),
/// so keeping a subscription in a scope guarantees it is released when that
/// scope ends.
pub struct Subscription<T> {
    id: u64,
    rx: mpsc::Receiver<T>,
    broker: Weak<Mutex<BrokerInner<T>>>,
}

impl<T> Subscriber<T> for Subscription<T> {
    fn try_recv(&self) -> Result<T, TryRecvError> {
        self.rx.try_recv()
    }

    fn recv(&self) -> Result<T, mpsc::RecvError> {
        self.rx.recv()
    }
}

impl<T> Subscription<T> {
    /// The subscriber id assigned by the broker (0 for a subscription taken
    /// out after [`Broker::shutdown`]).
    pub fn id(&self) -> u64 {
        self.id
    }

    /// Receive the next event without blocking.
    pub fn try_recv(&self) -> Result<T, TryRecvError> {
        self.rx.try_recv()
    }

    /// Block until an event is available. Returns [`std::sync::mpsc::RecvError`]
    /// once no more events can ever arrive.
    pub fn recv(&self) -> Result<T, mpsc::RecvError> {
        self.rx.recv()
    }

    /// Iterator over buffered events (drains the channel).
    pub fn iter(&self) -> mpsc::Iter<'_, T> {
        self.rx.iter()
    }
}

impl<T> Drop for Subscription<T> {
    fn drop(&mut self) {
        if let Some(inner) = self.broker.upgrade() {
            inner.lock().unwrap().subscribers.remove(&self.id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fans_out_to_multiple_subscribers() {
        let broker: Broker<u32> = Broker::new();
        let a = broker.subscribe(8);
        let b = broker.subscribe(8);
        assert_eq!(broker.subscriber_count(), 2);

        let delivered = broker.publish(41);
        assert_eq!(delivered, 2);
        assert_eq!(a.try_recv().unwrap(), 41);
        assert_eq!(b.try_recv().unwrap(), 41);
        assert!(matches!(a.try_recv(), Err(TryRecvError::Empty)));
    }

    #[test]
    fn subscription_cleans_up_on_drop() {
        let broker: Broker<String> = Broker::new();
        {
            let _sub = broker.subscribe(4);
            assert_eq!(broker.subscriber_count(), 1);
        }
        assert_eq!(broker.subscriber_count(), 0);
        assert_eq!(broker.publish("goes nowhere".to_string()), 0);
    }

    #[test]
    fn explicit_remove_deregisters_by_id() {
        let broker: Broker<u8> = Broker::new();
        let a = broker.subscribe(2);
        let b = broker.subscribe(2);
        broker.remove(a.id());
        assert_eq!(broker.subscriber_count(), 1);
        assert_eq!(broker.publish(7), 1);
        assert!(matches!(a.try_recv(), Err(TryRecvError::Disconnected)));
        assert_eq!(b.try_recv().unwrap(), 7);
    }

    #[test]
    fn full_buffer_skips_event_but_keeps_subscriber() {
        let broker: Broker<u8> = Broker::new();
        let sub = broker.subscribe(2); // buffer capacity 2
        broker.publish(1);
        broker.publish(2);
        // Buffer now full → this event is dropped for the subscriber.
        assert_eq!(broker.publish(3), 0);
        assert_eq!(broker.subscriber_count(), 1);
        assert_eq!(sub.try_recv().unwrap(), 1);
        assert_eq!(sub.try_recv().unwrap(), 2);
        assert!(matches!(sub.try_recv(), Err(TryRecvError::Empty)));
    }

    #[test]
    fn shutdown_closes_all_subscriber_channels() {
        let broker: Broker<u8> = Broker::new();
        let a = broker.subscribe(4);
        let b = broker.subscribe(4);
        broker.publish(9);
        assert_eq!(a.try_recv().unwrap(), 9);

        broker.shutdown();
        assert!(broker.is_closed());
        assert_eq!(broker.subscriber_count(), 0);
        assert_eq!(broker.publish(1), 0, "no deliver to closed channels");
        // Buffered values are still readable, but the channel is disconnected
        // afterward (the sender side was dropped on shutdown).
        assert_eq!(b.try_recv().unwrap(), 9);
        assert!(matches!(a.try_recv(), Err(TryRecvError::Disconnected)));
        assert!(matches!(b.try_recv(), Err(TryRecvError::Disconnected)));
    }

    #[test]
    fn subscribe_after_shutdown_is_disconnected() {
        let broker: Broker<String> = Broker::new();
        broker.shutdown();
        let sub = broker.subscribe(4);
        assert!(matches!(sub.try_recv(), Err(TryRecvError::Disconnected)));
        assert_eq!(broker.publish("nope".to_string()), 0);
    }

    #[test]
    fn publisher_and_subscriber_traits_are_available() {
        fn via_publisher<P: Publisher<u32>>(p: &P) -> usize {
            p.publish(11)
        }
        fn via_subscriber<S: Subscriber<u32>>(s: &S) -> u32 {
            s.try_recv().unwrap()
        }

        let broker: Broker<u32> = Broker::new();
        let sub = broker.subscribe(4);
        assert_eq!(via_publisher(&broker), 1);
        assert_eq!(via_subscriber(&sub), 11);
    }
}