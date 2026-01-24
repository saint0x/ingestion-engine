//! Partition routing for events.

use std::hash::{Hash, Hasher};

/// Computes a partition key hash for consistent routing.
pub fn partition_hash(key: &str, num_partitions: i32) -> i32 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    key.hash(&mut hasher);
    let hash = hasher.finish();
    (hash % num_partitions as u64) as i32
}

/// Partition strategy.
#[derive(Debug, Clone, Copy, Default)]
pub enum PartitionStrategy {
    /// Partition by session ID (maintains ordering per session)
    #[default]
    BySession,
    /// Partition by tenant ID (all tenant events in same partition)
    ByTenant,
    /// Round-robin (no ordering guarantees)
    RoundRobin,
}

/// Returns the partition key based on strategy.
pub fn get_partition_key(
    strategy: PartitionStrategy,
    session_id: &str,
    tenant_id: &str,
) -> Option<String> {
    match strategy {
        PartitionStrategy::BySession => Some(session_id.to_string()),
        PartitionStrategy::ByTenant => Some(tenant_id.to_string()),
        PartitionStrategy::RoundRobin => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_consistent_hashing() {
        let key = "session-123";
        let partitions = 12;

        // Same key should always produce same partition
        let p1 = partition_hash(key, partitions);
        let p2 = partition_hash(key, partitions);
        assert_eq!(p1, p2);

        // Partition should be in valid range
        assert!(p1 >= 0 && p1 < partitions);
    }
}
