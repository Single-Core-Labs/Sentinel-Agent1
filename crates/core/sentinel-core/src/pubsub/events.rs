//! Foundational resource-lifecycle event types.
//!
//! These generic payload wrappers are what a [`Broker`][super::Broker]
//! carries to signal that a resource was created, updated, or deleted. Any
//! component can subscribe to a broker typed to one of these and react to
//! state changes in real time.

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
        Self { id: id.into(), value }
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
        Self { id: id.into(), value }
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