//! Foundational resource-lifecycle event types.
//!
//! These generic payload wrappers are what a [`Broker`][super::Broker]
//! carries to signal that a resource was created, updated, or deleted. Any
//! component can subscribe to a broker typed to one of these and react to
//! state changes in real time.
//!
//! [`EventType`] is the machine-readable category string and [`Event`] is a
//! generic, type-plus-payload carrier for strong typing across producers and
//! consumers.

/// The machine-readable type of an event (the reference `EventType` string).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EventType {
    Created,
    Updated,
    Deleted,
}

impl EventType {
    /// Stable lowercase category string: `"created"`, `"updated"`, `"deleted"`.
    pub fn as_str(self) -> &'static str {
        match self {
            EventType::Created => "created",
            EventType::Updated => "updated",
            EventType::Deleted => "deleted",
        }
    }

    /// Parse a category string (case-insensitive). Unknown names → `None`.
    pub fn parse(s: &str) -> Option<EventType> {
        match s.to_ascii_lowercase().as_str() {
            "created" => Some(EventType::Created),
            "updated" => Some(EventType::Updated),
            "deleted" => Some(EventType::Deleted),
            _ => None,
        }
    }
}

/// A generic event: a category [`EventType`] paired with a typed payload
/// (the reference generic `Event` struct). Use it for brokers where the
/// category must be carried alongside the value — the wide counterpart to
/// [`CreatedEvent`]/[`UpdatedEvent`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event<T> {
    event_type: EventType,
    payload: T,
}

impl<T> Event<T> {
    /// Build an event of `event_type` carrying `payload`.
    pub fn new(event_type: EventType, payload: T) -> Self {
        Self {
            event_type,
            payload,
        }
    }

    /// The event's category.
    pub fn event_type(&self) -> EventType {
        self.event_type
    }

    /// The typed payload.
    pub fn payload(&self) -> &T {
        &self.payload
    }

    /// Consume the event, returning the payload.
    pub fn into_payload(self) -> T {
        self.payload
    }
}

impl<T> From<CreatedEvent<T>> for Event<T> {
    fn from(e: CreatedEvent<T>) -> Self {
        Event::new(EventType::Created, e.into_value())
    }
}

impl<T> From<UpdatedEvent<T>> for Event<T> {
    fn from(e: UpdatedEvent<T>) -> Self {
        Event::new(EventType::Updated, e.into_value())
    }
}

/// A resource was created, carrying its id and current value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreatedEvent<T> {
    id: String,
    value: T,
}

/// A resource was updated, carrying its id and the new value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdatedEvent<T> {
    id: String,
    value: T,
}

/// A resource was deleted, carrying only its id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeletedEvent<T> {
    id: String,
    #[doc(hidden)]
    _marker: std::marker::PhantomData<T>,
}

/// A tagged union over the three lifecycle events, for brokers that observe
/// an entire lifecycle on a single channel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LifecycleEvent<T> {
    Created(CreatedEvent<T>),
    Updated(UpdatedEvent<T>),
    Deleted(DeletedEvent<T>),
}

impl<T> CreatedEvent<T> {
    pub fn new(id: impl Into<String>, value: T) -> Self {
        Self {
            id: id.into(),
            value,
        }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn value(&self) -> &T {
        &self.value
    }

    pub fn into_value(self) -> T {
        self.value
    }
}

impl<T> UpdatedEvent<T> {
    pub fn new(id: impl Into<String>, value: T) -> Self {
        Self {
            id: id.into(),
            value,
        }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn value(&self) -> &T {
        &self.value
    }

    pub fn into_value(self) -> T {
        self.value
    }
}

impl<T> DeletedEvent<T> {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            _marker: std::marker::PhantomData,
        }
    }

    pub fn id(&self) -> &str {
        &self.id
    }
}

impl<T> LifecycleEvent<T> {
    /// A stable machine-readable kind: `"created"`, `"updated"`, `"deleted"`.
    pub fn kind(&self) -> &'static str {
        match self {
            LifecycleEvent::Created(_) => "created",
            LifecycleEvent::Updated(_) => "updated",
            LifecycleEvent::Deleted(_) => "deleted",
        }
    }

    /// The `id` field shared by every variant.
    pub fn id(&self) -> &str {
        match self {
            LifecycleEvent::Created(e) => e.id(),
            LifecycleEvent::Updated(e) => e.id(),
            LifecycleEvent::Deleted(e) => e.id(),
        }
    }
}

impl<T> From<CreatedEvent<T>> for LifecycleEvent<T> {
    fn from(e: CreatedEvent<T>) -> Self {
        LifecycleEvent::Created(e)
    }
}

impl<T> From<UpdatedEvent<T>> for LifecycleEvent<T> {
    fn from(e: UpdatedEvent<T>) -> Self {
        LifecycleEvent::Updated(e)
    }
}

impl<T> From<DeletedEvent<T>> for LifecycleEvent<T> {
    fn from(e: DeletedEvent<T>) -> Self {
        LifecycleEvent::Deleted(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_type_strings_roundtrip() {
        assert_eq!(EventType::Created.as_str(), "created");
        assert_eq!(EventType::Updated.as_str(), "updated");
        assert_eq!(EventType::Deleted.as_str(), "deleted");
        assert_eq!(EventType::parse("CREATED"), Some(EventType::Created));
        assert_eq!(EventType::parse("bogus"), None);
    }

    #[test]
    fn generic_event_carries_type_and_payload() {
        let created = Event::new(EventType::Created, 42u32);
        assert_eq!(created.event_type(), EventType::Created);
        assert_eq!(created.payload(), &42);
        assert_eq!(created.into_payload(), 42);

        let event = Event::from(UpdatedEvent::new("file-1", "new-content"));
        assert_eq!(event.event_type(), EventType::Updated);
        assert_eq!(event.into_payload(), "new-content");
    }
}
