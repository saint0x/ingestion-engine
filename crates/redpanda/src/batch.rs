//! Event batch accumulator.

use engine_core::Event;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// A batch of events for a specific topic.
#[derive(Debug)]
pub struct EventBatch {
    pub topic: String,
    pub events: Vec<Event>,
    pub created_at: Instant,
}

impl EventBatch {
    pub fn new(topic: impl Into<String>) -> Self {
        Self {
            topic: topic.into(),
            events: Vec::new(),
            created_at: Instant::now(),
        }
    }

    pub fn push(&mut self, event: Event) {
        self.events.push(event);
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    pub fn age(&self) -> Duration {
        self.created_at.elapsed()
    }

    /// Take events and reset the batch.
    pub fn take(&mut self) -> Vec<Event> {
        self.created_at = Instant::now();
        std::mem::take(&mut self.events)
    }
}

/// Batch accumulator configuration.
#[derive(Debug, Clone)]
pub struct BatchConfig {
    /// Maximum batch size before flush
    pub max_size: usize,
    /// Maximum batch age before flush
    pub max_age: Duration,
}

impl Default for BatchConfig {
    fn default() -> Self {
        Self {
            max_size: 1000,
            max_age: Duration::from_millis(100),
        }
    }
}

/// Accumulates events into batches per topic.
pub struct BatchAccumulator {
    config: BatchConfig,
    batches: Mutex<HashMap<String, EventBatch>>,
}

impl BatchAccumulator {
    pub fn new(config: BatchConfig) -> Self {
        Self {
            config,
            batches: Mutex::new(HashMap::new()),
        }
    }

    /// Add an event to the appropriate batch.
    /// Returns the batch if it should be flushed.
    pub fn add(&self, event: Event) -> Option<EventBatch> {
        let topic = event.payload.topic().to_string();
        let mut batches = self.batches.lock();

        let batch = batches
            .entry(topic.clone())
            .or_insert_with(|| EventBatch::new(&topic));

        batch.push(event);

        // Check if batch should be flushed
        if batch.len() >= self.config.max_size {
            let events = batch.take();
            return Some(EventBatch {
                topic,
                events,
                created_at: Instant::now(),
            });
        }

        None
    }

    /// Flush all batches that have exceeded max age.
    pub fn flush_aged(&self) -> Vec<EventBatch> {
        let mut batches = self.batches.lock();
        let mut flushed = Vec::new();

        for batch in batches.values_mut() {
            if batch.age() >= self.config.max_age && !batch.is_empty() {
                let events = batch.take();
                flushed.push(EventBatch {
                    topic: batch.topic.clone(),
                    events,
                    created_at: Instant::now(),
                });
            }
        }

        flushed
    }

    /// Flush all batches regardless of size or age.
    pub fn flush_all(&self) -> Vec<EventBatch> {
        let mut batches = self.batches.lock();
        let mut flushed = Vec::new();

        for batch in batches.values_mut() {
            if !batch.is_empty() {
                let events = batch.take();
                flushed.push(EventBatch {
                    topic: batch.topic.clone(),
                    events,
                    created_at: Instant::now(),
                });
            }
        }

        flushed
    }
}
