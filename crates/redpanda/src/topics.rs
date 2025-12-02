//! Topic definitions for event types.

/// Topic names for each event type.
pub mod topic {
    pub const PAGEVIEW: &str = "events_pageview";
    pub const CLICK: &str = "events_click";
    pub const SCROLL: &str = "events_scroll";
    pub const PERFORMANCE: &str = "events_performance";
    pub const CUSTOM: &str = "events_custom";

    /// All topics for initialization.
    pub const ALL: &[&str] = &[PAGEVIEW, CLICK, SCROLL, PERFORMANCE, CUSTOM];
}

/// Topic configuration.
#[derive(Debug, Clone)]
pub struct TopicConfig {
    pub name: &'static str,
    pub partitions: i32,
    pub replication_factor: i32,
    pub retention_ms: i64,
}

impl TopicConfig {
    pub const fn new(name: &'static str) -> Self {
        Self {
            name,
            partitions: 12,
            replication_factor: 3,
            retention_ms: 7 * 24 * 60 * 60 * 1000, // 7 days
        }
    }

    pub const fn with_partitions(mut self, partitions: i32) -> Self {
        self.partitions = partitions;
        self
    }

    pub const fn with_replication(mut self, factor: i32) -> Self {
        self.replication_factor = factor;
        self
    }

    pub const fn with_retention_ms(mut self, ms: i64) -> Self {
        self.retention_ms = ms;
        self
    }
}

/// Default topic configurations.
pub fn default_topic_configs() -> Vec<TopicConfig> {
    vec![
        TopicConfig::new(topic::PAGEVIEW).with_partitions(12),
        TopicConfig::new(topic::CLICK).with_partitions(12),
        TopicConfig::new(topic::SCROLL).with_partitions(6),
        TopicConfig::new(topic::PERFORMANCE).with_partitions(6),
        TopicConfig::new(topic::CUSTOM).with_partitions(12),
    ]
}
