# ClickHouse Self-Hosting Guide

Operational guide for self-hosting ClickHouse with the Overwatch ingestion engine.

> This guide captures critical operational lessons. If you're self-hosting ClickHouse instead of using ClickHouse Cloud, internalize these rules.

---

## Quick Assessment: Current State

| Area | Status | Notes |
|------|--------|-------|
| Batching | OK | 5000 events/batch, 500ms timeout |
| Schema Design | OK | Append-only, no mutations, JSON blob for extensibility |
| TTL Strategy | OK | Partition-level deletion via retention worker |
| Monitoring | OK | system.parts/merges/disks queries with alerting |
| Notifications | OK | Webhook support via `INGESTION_NOTIFICATION_WEBHOOK_URL` |
| Logging | OK | JSON logging via `LOG_JSON=1`, stdout |
| Backups | Manual | Use clickhouse-backup or partition exports |

---

## 1. Ingestion: Batch Configuration

**Current Settings** (`config/default.toml`):
```toml
[redpanda]
batch_size = 5000           # Events per batch
batch_timeout_ms = 500      # Flush timeout
compression = "lz4"         # Good for throughput
```

**Why This Matters**:
- Small batches -> too many parts -> merge pressure -> cluster death
- Every insert creates parts; ClickHouse must merge them
- Failure mode is gradual: works fine until traffic grows, then cascades

**Self-Hosting Rule**: If merge time is trending up, increase batch sizes.

---

## 2. TTL Strategy: Partition-Level Deletion

### Implementation

Instead of row-level TTL (which causes continuous background mutations), the ingestion engine uses **partition-level deletion**:

1. Tables are partitioned by `toYYYYMM(timestamp)` (monthly)
2. The retention worker runs hourly
3. It queries `system.parts` to find partitions older than retention period
4. It drops entire partitions with `ALTER TABLE ... DROP PARTITION`

### Benefits

- No background mutations (row-level TTL constantly rewrites parts)
- Instant, atomic deletes
- Predictable resource usage
- Better for backups (partitions are immutable until dropped)

### Retention Periods

| Data Type | Retention |
|-----------|-----------|
| Analytics events | 90 days (~3 months) |
| Internal metrics | 30 days (~1 month) |

### Migration from Row-Level TTL

If upgrading from an older version with row-level TTL:

```rust
// Run this once during upgrade
clickhouse_client::schema::migrate_remove_ttl(&client).await?;
```

Or manually via SQL:
```sql
ALTER TABLE overwatch.events REMOVE TTL;
-- Repeat for all 14 tables
```

---

## 3. Monitoring for Self-Hosting

### Automatic Monitoring

The ingestion engine automatically collects ClickHouse operational metrics every 60 seconds:

- **Parts count** per table from `system.parts`
- **Active merges** from `system.merges`
- **Disk usage** from `system.disks`
- **Merge pressure score** (0-100) calculated from above

### Alert Thresholds

| Metric | Warning | Critical |
|--------|---------|----------|
| Merge pressure score | > 40 | > 70 |
| Parts per table | > 300 | > 1000 |
| Active merges | > 5 | > 10 |
| Disk usage | > 70% | > 85% |
| Merge duration | > 60s | > 300s |

### Manual Queries

Check merge pressure manually:
```sql
-- Parts count per table (should be < 300)
SELECT database, table, count() as parts, sum(rows) as rows
FROM system.parts
WHERE database = 'overwatch' AND active = 1
GROUP BY database, table
ORDER BY parts DESC;

-- Active merges (should be < 10)
SELECT database, table, elapsed, progress, num_parts
FROM system.merges
WHERE database = 'overwatch'
ORDER BY elapsed DESC;

-- Disk usage
SELECT name, path,
       formatReadableSize(free_space) as free,
       formatReadableSize(total_space) as total,
       round(100 - (free_space / total_space * 100), 2) as used_pct
FROM system.disks;
```

### Log-Based Monitoring

With `LOG_JSON=1`, grep for these patterns:
```bash
# Critical merge pressure
grep "CRITICAL.*merge pressure" /var/log/overwatch-ingestion.log

# Disk warnings
grep "Disk usage high" /var/log/overwatch-ingestion.log

# ClickHouse insert errors
grep "clickhouse_insert_errors" /var/log/overwatch-ingestion.log
```

---

## 4. Notifications

### Configuring Webhooks

Set environment variable:
```bash
export INGESTION_NOTIFICATION_WEBHOOK_URL=https://your-webhook.example.com/alerts
```

### Webhook Payload Format

```json
{
  "timestamp": "2024-01-15T12:00:00Z",
  "source": "overwatch-ingestion",
  "notification": {
    "type": "merge_pressure",
    "score": 75.5,
    "max_parts": 850,
    "active_merges": 8
  }
}
```

### Notification Types

| Type | Trigger |
|------|---------|
| `system_alert` | High error rate (>10%), backpressure active |
| `merge_pressure` | Merge pressure score > 70 |
| `disk_usage` | Disk usage > 85% |

---

## 5. Backup Strategy

### Option 1: clickhouse-backup (Recommended)

```bash
# Install
wget https://github.com/AlexAkulov/clickhouse-backup/releases/download/v2.4.0/clickhouse-backup-linux-amd64.tar.gz
tar xf clickhouse-backup-linux-amd64.tar.gz
sudo mv clickhouse-backup /usr/local/bin/

# Create backup
clickhouse-backup create overwatch_$(date +%Y%m%d)

# List backups
clickhouse-backup list

# Restore
clickhouse-backup restore overwatch_20240115
```

### Option 2: S3-Compatible Storage (Hetzner Object Storage)

Configure ClickHouse for backup to S3:
```xml
<!-- /etc/clickhouse-server/config.d/backup.xml -->
<clickhouse>
    <backups>
        <allowed_path>/backups/</allowed_path>
        <allowed_disk>backups</allowed_disk>
    </backups>
    <storage_configuration>
        <disks>
            <backups>
                <type>s3</type>
                <endpoint>https://your-bucket.fsn1.your-objectstorage.com/clickhouse-backups/</endpoint>
                <access_key_id>your-key</access_key_id>
                <secret_access_key>your-secret</secret_access_key>
            </backups>
        </disks>
    </storage_configuration>
</clickhouse>
```

Then backup with:
```sql
BACKUP TABLE overwatch.events TO Disk('backups', 'events_20240115.zip');
```

### Backup Schedule

Recommended for production:
- Full backup: Weekly (Sunday 3 AM)
- Partition export: Daily (3 AM)
- Keep: 4 weekly + 7 daily

---

## 6. Logging Configuration

### Environment Variables

```bash
# Enable JSON logging (recommended for production)
LOG_JSON=1

# Log level configuration
RUST_LOG=info,clickhouse_client=debug,worker=debug
```

### Systemd Service (Hetzner)

```ini
# /etc/systemd/system/overwatch-ingestion.service
[Unit]
Description=Overwatch Ingestion Engine
After=network.target

[Service]
Type=simple
User=overwatch
Environment="LOG_JSON=1"
Environment="RUST_LOG=info,clickhouse_client=debug"
Environment="INGESTION_CLICKHOUSE_URL=http://localhost:8123"
Environment="INGESTION_REDPANDA_BROKERS=localhost:9092"
ExecStart=/opt/overwatch/ingestion-engine
Restart=always
RestartSec=5

[Install]
WantedBy=multi-user.target
```

### Log Rotation

```bash
# /etc/logrotate.d/overwatch
/var/log/overwatch/*.log {
    daily
    rotate 14
    compress
    delaycompress
    missingok
    notifempty
    create 0640 overwatch overwatch
}
```

---

## 7. Hetzner-Specific Recommendations

### Server Sizing

| Traffic | Server | ClickHouse Specs |
|---------|--------|------------------|
| < 1M events/day | CX21 | 4GB RAM, 40GB NVMe |
| 1-10M events/day | CX31 | 8GB RAM, 80GB NVMe |
| 10-100M events/day | CX41 | 16GB RAM, 160GB NVMe |
| > 100M events/day | Dedicated | 64GB+ RAM, RAID NVMe |

### Network Tuning

```bash
# Enable jumbo frames for internal network (if using private network)
ip link set eth1 mtu 9000

# Tune TCP for high throughput
sysctl -w net.core.rmem_max=134217728
sysctl -w net.core.wmem_max=134217728
sysctl -w net.ipv4.tcp_rmem="4096 87380 67108864"
sysctl -w net.ipv4.tcp_wmem="4096 65536 67108864"
```

### ClickHouse Memory Settings

```xml
<!-- /etc/clickhouse-server/config.d/memory.xml -->
<clickhouse>
    <max_server_memory_usage_to_ram_ratio>0.8</max_server_memory_usage_to_ram_ratio>
    <max_memory_usage>4000000000</max_memory_usage> <!-- 4GB per query -->
    <max_bytes_before_external_group_by>2000000000</max_bytes_before_external_group_by>
</clickhouse>
```

### Firewall Rules

```bash
# ClickHouse HTTP (internal only)
ufw allow from 10.0.0.0/8 to any port 8123

# ClickHouse native (internal only)
ufw allow from 10.0.0.0/8 to any port 9000

# Redpanda (internal only)
ufw allow from 10.0.0.0/8 to any port 9092

# Ingestion API (public)
ufw allow 8080/tcp
```

---

## 8. Troubleshooting

### High Parts Count

**Symptom**: Parts count per table exceeds 300
**Cause**: Batch size too small, high insert frequency
**Fix**:
1. Increase `batch_size` in config (default: 5000)
2. Increase `batch_timeout_ms` (default: 500)
3. Check for multiple ingestion instances hitting same tables

### Merge Lag

**Symptom**: Active merges > 10, merge duration > 60s
**Cause**: Parts creation faster than merge throughput
**Fix**:
1. Increase batch sizes
2. Add more CPU/RAM to ClickHouse
3. Check for slow disks

### Disk Full

**Symptom**: Disk usage > 90%
**Cause**: Data retention too long, backup accumulation
**Fix**:
1. Run retention worker manually: check logs for partition drops
2. Clear old backups
3. Verify TTL migration completed (no row-level TTL causing rewrites)

### High Memory Usage

**Symptom**: ClickHouse OOM kills
**Cause**: Large queries, insufficient limits
**Fix**:
1. Set `max_memory_usage` per query
2. Add `max_bytes_before_external_group_by` for large aggregations
3. Check for runaway queries in `system.processes`

---

## 9. Quick Reference

### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `INGESTION_HOST` | 0.0.0.0 | Listen address |
| `INGESTION_PORT` | 8080 | Listen port |
| `INGESTION_AUTH_URL` | mock | Auth service URL |
| `INGESTION_CLICKHOUSE_URL` | http://localhost:8123 | ClickHouse URL |
| `INGESTION_CLICKHOUSE_DATABASE` | overwatch | Database name |
| `INGESTION_CLICKHOUSE_USERNAME` | - | ClickHouse user |
| `INGESTION_CLICKHOUSE_PASSWORD` | - | ClickHouse password |
| `INGESTION_REDPANDA_BROKERS` | localhost:9092 | Kafka brokers |
| `INGESTION_REDPANDA_SASL_USERNAME` | - | SASL username |
| `INGESTION_REDPANDA_SASL_PASSWORD` | - | SASL password |
| `INGESTION_NOTIFICATION_WEBHOOK_URL` | - | Webhook for alerts |
| `LOG_JSON` | false | Enable JSON logging |
| `RUST_LOG` | info | Log level filter |

### Health Endpoints

| Endpoint | Purpose |
|----------|---------|
| `GET /health` | Full health status with components |
| `GET /health/ready` | Kubernetes readiness probe |
| `GET /health/live` | Kubernetes liveness probe |

### Useful ClickHouse Queries

```sql
-- Current ingestion rate (last hour)
SELECT
    count() / 3600 as events_per_second
FROM overwatch.events
WHERE timestamp > now() - INTERVAL 1 HOUR;

-- Storage per table
SELECT
    table,
    formatReadableSize(sum(bytes_on_disk)) as size,
    sum(rows) as rows
FROM system.parts
WHERE database = 'overwatch' AND active
GROUP BY table
ORDER BY sum(bytes_on_disk) DESC;

-- Partition sizes
SELECT
    table,
    partition,
    formatReadableSize(sum(bytes_on_disk)) as size
FROM system.parts
WHERE database = 'overwatch' AND active
GROUP BY table, partition
ORDER BY table, partition;
```
