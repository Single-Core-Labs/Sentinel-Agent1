//! Generic publish/subscribe event infrastructure.
//!
//! [`Broker`] is a thread-safe, generic hub that dispatches values of any
//! payload type to buffered per-subscriber channels. Subscriptions are
//! cleaned up automatically when they go out of scope (RAII), and explicitly
//! via [`Subscription`]'s id, so components can be tied to a context lifetime.
//!
//! [`events`] defines the foundational resource-lifecycle event types
//! (`Created`, `Updated`, `Deleted`) that other components publish on a
//! broker to signal state changes in real time.

pub mod broker;
pub mod events;

pub use broker::{Broker, Subscription};
pub use events::{CreatedEvent, DeletedEvent, LifecycleEvent, UpdatedEvent};