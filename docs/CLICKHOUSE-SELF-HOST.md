# ClickHouse Self-Hosting Guide

Operational guide for self-hosting ClickHouse with the Overwatch ingestion engine.

> This guide captures critical operational lessons. If you're self-hosting ClickHouse instead of using ClickHouse Cloud, internalize these rules.

---

## Quick Assessment: Current State

| Area | Status | Notes |
|------|--------|-------|
| Batching | ✅ Good | 1000 events/batch default, configurable |
| Schema Design | ✅ Good | Append-only, no mutations, JSON blob for extensibility |
| TTL Strategy | ⚠️ Review | Row-level TTL configured; consider partition drops |
| Monitoring | ⚠️ Incomplete | Internal metrics only; need host + CH metrics |
| Logging | ⚠️ Manual | Logs to stdout; configure external shipping |
| Backups | ⚠️ Not configured | Need backup strategy before production |

---

## 1. Ingestion: Batch Configuration

**Current Settings** (`config/default.toml`):
```toml
[redpanda]
batch_size = 1000          # Events per batch
batch_timeout_ms = 100     # Flush timeout
compression = "lz4"        # Good for throughput
```

**Why This Matters**:
- Small batches → too many parts → merge pressure → cluster death
- Every insert creates parts; ClickHouse must merge them
- Failure mode is gradual: works fine until traffic grows, then cascades

**Production Recommendations**:
```toml
[redpanda]
batch_size = 5000          # Larger batches for high throughput
batch_timeout_ms = 500     # Allow more time to accumulate
compression = "lz4"        # Keep lz4 for speed
```

**Self-Hosting Rule**: If merge time is trending up, increase batch sizes.

---

## 2. TTL Strategy: Row-Level vs Partition-Level

### Current Implementation

All tables use row-level TTL:
```sql
TTL toDateTime(timestamp) + INTERVAL 90 DAY
```

### The Problem

Row-level TTLs cause continuous background mutations:
- Each file contains rows with different TTLs
- ClickHouse constantly rewrites parts to delete expired rows
- Adds non-obvious performance overhead

### Recommended Pattern: Partition-Level Deletion

Tables are already partitioned by month:
```sql
PARTITION BY toYYYYMM(timestamp)
```

**Better approach**: Scheduled partition drops instead of row-level TTL.

**Scheduled Job (Cron/K8s CronJob)**:
```bash
#!/bin/bash
# Run daily at 2am

# Calculate partition to drop (e.g., 90 days ago)
PARTITION_TO_DROP=$(date -d "90 days ago" +%Y%m)

# Drop old partitions from all tables
clickhouse-client --query "
  ALTER TABLE overwatch.events DROP PARTITION '$PARTITION_TO_DROP';
  ALTER TABLE overwatch.sessions DROP PARTITION '$PARTITION_TO_DROP';
  ALTER TABLE overwatch.pageviews DROP PARTITION '$PARTITION_TO_DROP';
  ALTER TABLE overwatch.clicks DROP PARTITION '$PARTITION_TO_DROP';
  ALTER TABLE overwatch.scroll_events DROP PARTITION '$PARTITION_TO_DROP';
  ALTER TABLE overwatch.mouse_moves DROP PARTITION '$PARTITION_TO_DROP';
  ALTER TABLE overwatch.form_events DROP PARTITION '$PARTITION_TO_DROP';
  ALTER TABLE overwatch.errors DROP PARTITION '$PARTITION_TO_DROP';
  ALTER TABLE overwatch.performance_metrics DROP PARTITION '$PARTITION_TO_DROP';
  ALTER TABLE overwatch.visibility_events DROP PARTITION '$PARTITION_TO_DROP';
  ALTER TABLE overwatch.resource_loads DROP PARTITION '$PARTITION_TO_DROP';
  ALTER TABLE overwatch.geographic DROP PARTITION '$PARTITION_TO_DROP';
  ALTER TABLE overwatch.custom_events DROP PARTITION '$PARTITION_TO_DROP';
"
```

**Migration Path**:
1. Keep row-level TTL for initial deployment (simpler)
2. Monitor merge performance
3. If merge time increases, switch to partition drops
4. Remove TTL from schemas when switching

---

## 3. ClickHouse Configuration Tuning

### Default Settings (Often Too Conservative)

ClickHouse ships with safe defaults that need tuning for real workloads.

**Add to ClickHouse config** (`/etc/clickhouse-server/config.d/tuning.xml`):

```xml
<clickhouse>
    <!-- Increase concurrent query limit -->
    <max_concurrent_queries>200</max_concurrent_queries>

    <!-- Keep-alive for high-throughput ingestion -->
    <keep_alive_timeout>300</keep_alive_timeout>

    <!-- Allow larger partition operations -->
    <max_partition_size_to_drop>100000000000</max_partition_size_to_drop>
    <max_table_size_to_drop>100000000000</max_table_size_to_drop>

    <!-- Insert settings -->
    <max_insert_threads>8</max_insert_threads>

    <!-- Memory limits -->
    <max_memory_usage>10000000000</max_memory_usage>
    <max_memory_usage_for_all_queries>20000000000</max_memory_usage_for_all_queries>
</clickhouse>
```

**User Profile Settings** (`/etc/clickhouse-server/users.d/profiles.xml`):
```xml
<clickhouse>
    <profiles>
        <ingestion>
            <max_execution_time>300</max_execution_time>
            <max_insert_block_size>1048576</max_insert_block_size>
            <max_threads>8</max_threads>
        </ingestion>
    </profiles>
</clickhouse>
```

### Docker Compose Override

Update `docker-compose.yml` for production:
```yaml
clickhouse:
  image: clickhouse/clickhouse-server:24.1
  ports:
    - "8123:8123"
    - "9000:9000"
  volumes:
    - clickhouse_data:/var/lib/clickhouse
    - ./config/clickhouse/config.d:/etc/clickhouse-server/config.d
    - ./config/clickhouse/users.d:/etc/clickhouse-server/users.d
  ulimits:
    nofile:
      soft: 262144
      hard: 262144
  environment:
    - CLICKHOUSE_DB=overwatch
```

---

## 4. Disk Management

### Critical Rule: Check Disk First

If anything looks weird, **check disk first**. Always.

Disk exhaustion causes:
- Partial insert success
- Replication traffic explosion
- Cluster behaving irrationally

### Monitoring Requirements

```sql
-- Check disk usage
SELECT
    name,
    path,
    formatReadableSize(free_space) AS free,
    formatReadableSize(total_space) AS total,
    round(free_space / total_space * 100, 2) AS free_percent
FROM system.disks;

-- Check per-table disk usage
SELECT
    database,
    table,
    formatReadableSize(sum(bytes_on_disk)) AS size,
    sum(rows) AS rows
FROM system.parts
WHERE active
GROUP BY database, table
ORDER BY sum(bytes_on_disk) DESC;
```

### Alert Thresholds

| Threshold | Action |
|-----------|--------|
| 85% used | Warning alert |
| 90% used | Critical alert |
| 95% used | Emergency: stop ingestion |

### AWS Disk Gotcha

AWS General Purpose SSDs cap at 16TB. Plan disk strategy:
- Local NVMe > EBS for performance
- Know your disk type limits
- Have a migration plan before you need it

---

## 5. Monitoring Setup

### A. Host/Box Metrics (Required)

Export to Prometheus/CloudWatch/Datadog:

| Metric | Why |
|--------|-----|
| Disk usage % | #1 cause of mysterious failures |
| Disk IO | Detect write saturation |
| CPU usage | Merge operations are CPU-intensive |
| Memory usage | OOM kills queries |
| Network in/out | Detect replication issues |

### B. ClickHouse Internal Metrics

ClickHouse exposes 800+ metrics. Key ones:

```sql
-- Parts and merge pressure
SELECT
    database,
    table,
    count() AS parts,
    sum(rows) AS rows,
    formatReadableSize(sum(bytes_on_disk)) AS size
FROM system.parts
WHERE active
GROUP BY database, table
ORDER BY parts DESC;

-- Active merges
SELECT * FROM system.merges;

-- Query performance
SELECT
    type,
    query_duration_ms,
    read_rows,
    written_rows,
    memory_usage
FROM system.query_log
WHERE event_date = today()
ORDER BY query_duration_ms DESC
LIMIT 20;
```

### Prometheus Exporter Setup

Add to docker-compose:
```yaml
clickhouse-exporter:
  image: f1yegor/clickhouse-exporter
  ports:
    - "9116:9116"
  environment:
    - CLICKHOUSE_URI=http://clickhouse:8123
  depends_on:
    - clickhouse
```

---

## 6. Logging Configuration

### Ship Logs Externally

Never debug ClickHouse using `tail` alone. Logs are huge and issues often happened hours ago.

**Option 1: CloudWatch (AWS)**
```yaml
# docker-compose with awslogs driver
clickhouse:
  logging:
    driver: awslogs
    options:
      awslogs-group: /clickhouse/server
      awslogs-region: us-east-1
      awslogs-stream-prefix: ch
```

**Option 2: Fluent Bit Sidecar**
```yaml
fluent-bit:
  image: fluent/fluent-bit
  volumes:
    - /var/lib/clickhouse/logs:/logs:ro
    - ./fluent-bit.conf:/fluent-bit/etc/fluent-bit.conf
```

### Key Logs to Export

- `/var/lib/clickhouse/logs/clickhouse-server.log` - Server logs
- `/var/lib/clickhouse/logs/clickhouse-server.err.log` - Errors
- Query logs via `system.query_log` table

---

## 7. Backup Strategy

### Minimum Viable Safety (3 Layers)

**Layer 1: Nightly ClickHouse Backups**
```bash
#!/bin/bash
# Run nightly

BACKUP_NAME="overwatch_$(date +%Y%m%d_%H%M%S)"

clickhouse-client --query "
  BACKUP DATABASE overwatch TO Disk('backups', '$BACKUP_NAME')
"

# Upload to S3
aws s3 sync /backups/$BACKUP_NAME s3://your-bucket/clickhouse-backups/$BACKUP_NAME
```

**Layer 2: Raw Event Backup (S3)**

Configure Redpanda/Kafka to mirror events to S3:
```yaml
# In Redpanda config
topic_retention_ms: 172800000  # 48h retention in Kafka
```

Plus a separate S3 sink connector for permanent storage.

**Layer 3: Streaming Buffer (Redpanda)**

Current Redpanda config provides some buffer. For production:
```yaml
redpanda:
  topic_retention_ms: 172800000    # 48 hours
  topic_retention_bytes: 10737418240  # 10GB per partition
```

### Recovery Expectations

| Scenario | Recovery Method | Data Loss |
|----------|-----------------|-----------|
| Bad query | Restore from nightly backup | Up to 24h |
| Disk failure | Restore from S3 raw events | Minimal |
| Corruption | Replay from Redpanda buffer | ~48h max |
| Total loss | Full restore from S3 | Depends on backup age |

---

## 8. Diagnostic Queries Playbook

### "2am Queries" - Copy and Keep Handy

**Check for Errors**:
```sql
SELECT
    event_time,
    name,
    value,
    last_error_message
FROM system.errors
WHERE event_date >= today() - 1
ORDER BY event_time DESC
LIMIT 50;
```

**Replication Status**:
```sql
SELECT
    database,
    table,
    is_leader,
    total_replicas,
    active_replicas,
    queue_size,
    inserts_in_queue,
    merges_in_queue
FROM system.replicas
WHERE queue_size > 0 OR inserts_in_queue > 0;
```

**Active Merges**:
```sql
SELECT
    database,
    table,
    elapsed,
    progress,
    num_parts,
    formatReadableSize(total_size_bytes_compressed) AS size,
    formatReadableSize(memory_usage) AS memory
FROM system.merges
ORDER BY elapsed DESC;
```

**Parts Count (Merge Pressure)**:
```sql
SELECT
    database,
    table,
    count() AS parts,
    sum(rows) AS total_rows,
    formatReadableSize(sum(bytes_on_disk)) AS total_size,
    min(modification_time) AS oldest_part,
    max(modification_time) AS newest_part
FROM system.parts
WHERE active
GROUP BY database, table
HAVING parts > 100
ORDER BY parts DESC;
```

**Slow Queries (Last 24h)**:
```sql
SELECT
    query_start_time,
    query_duration_ms,
    read_rows,
    read_bytes,
    result_rows,
    memory_usage,
    substring(query, 1, 200) AS query_preview
FROM system.query_log
WHERE event_date = today()
  AND type = 'QueryFinish'
  AND query_duration_ms > 1000
ORDER BY query_duration_ms DESC
LIMIT 20;
```

**Partition Sizes**:
```sql
SELECT
    database,
    table,
    partition,
    count() AS parts,
    sum(rows) AS rows,
    formatReadableSize(sum(bytes_on_disk)) AS size
FROM system.parts
WHERE active AND database = 'overwatch'
GROUP BY database, table, partition
ORDER BY table, partition;
```

**Check for Zombie Partitions (Bad Timestamps)**:
```sql
-- Data from far future or past indicates timestamp issues
SELECT
    table,
    partition,
    min(min_time) AS earliest,
    max(max_time) AS latest,
    sum(rows) AS rows
FROM system.parts
WHERE active AND database = 'overwatch'
GROUP BY table, partition
HAVING earliest < '2020-01-01' OR latest > now() + INTERVAL 1 YEAR
ORDER BY table, partition;
```

**System Health Overview**:
```sql
SELECT
    metric,
    value,
    description
FROM system.metrics
WHERE metric IN (
    'Query',
    'Merge',
    'ReplicatedFetch',
    'ReplicatedSend',
    'BackgroundPoolTask',
    'MemoryTracking'
);
```

---

## 9. Never Mutate Data

### This is Non-Negotiable

**Never run**:
```sql
ALTER TABLE ... MODIFY COLUMN ...
ALTER TABLE ... UPDATE ...
ALTER TABLE ... DELETE ...  -- row-level delete
```

On terabytes of data, this will:
- Spike CPU, memory, IO, network
- Stall inserts
- Break queries
- Potentially require cluster rebuild

### Safe Alternatives

| Need | Safe Approach |
|------|---------------|
| Add field | `ALTER TABLE ... ADD COLUMN` (instant) |
| Change field type | Create new table, dual-write, migrate |
| Delete old data | Drop partitions (instant) |
| Fix bad data | Let it age out via retention |

---

## 10. Production Checklist

Before going to production with self-hosted ClickHouse:

### Infrastructure
- [ ] Local NVMe or provisioned IOPS EBS (not GP2/GP3 for heavy workloads)
- [ ] Disk monitoring with alerts at 85%
- [ ] Ulimits configured (nofile: 262144+)
- [ ] Separate disks for data and logs (optional but recommended)

### Configuration
- [ ] ClickHouse config tuned (see Section 3)
- [ ] Batch sizes validated under load
- [ ] Keep-alive timeouts appropriate for workload

### Monitoring
- [ ] Host metrics exported (disk, CPU, memory, network)
- [ ] ClickHouse metrics exported (parts, merges, queries)
- [ ] Dashboards created for key metrics
- [ ] Alerts configured for critical thresholds

### Logging
- [ ] Logs shipped to external system
- [ ] Query logging enabled and retained
- [ ] Error log monitoring configured

### Backup & Recovery
- [ ] Nightly backup job running
- [ ] Backup restoration tested
- [ ] Raw event backup to S3 configured
- [ ] Streaming buffer retention configured (48h+)

### Operations
- [ ] Diagnostic queries playbook documented
- [ ] Partition cleanup job scheduled (if not using row TTL)
- [ ] Runbook for common failure scenarios
- [ ] On-call escalation path defined

---

## Summary

ClickHouse is incredibly powerful but unforgiving. Treat it as:
- An append-only analytical engine
- With disciplined ingestion (big batches)
- Aggressive observability (monitor everything)
- Zero tolerance for mutations

Follow these rules and it will run happily at massive scale.
