# Overwatch Ingestion Engine - Production Roadmap

High-throughput analytics ingestion pipeline: SDK events → Validation → Redpanda → ClickHouse

---

## Phase 1: Foundation (COMPLETE)

### Core Types & Validation (`crates/core/`)
- [x] Event type definitions (15 types: pageview, pageleave, click, scroll, form_*, error, visibility_change, resource_load, session_*, performance, custom)
- [x] SDK event schema (camelCase) with transform to ClickHouse format (snake_case)
- [x] Validation rules with `validator` crate
- [x] Unified error types with spec error codes (AUTH_001-005, VALID_001-003, DB_001, RATE_001)
- [x] Session management with 30-min timeout
- [x] API key format validation (`owk_(live|test)_[a-zA-Z0-9]{32}`)
- [x] Auth request/response types for daemon communication
- [x] Retention tier system (Free/Paid/Enterprise)
- [x] Memory limits (1MB batch, 64KB event, 16KB custom properties)

**Test:** `cargo test -p engine-core` - 17 tests covering events, auth, and transformation

---

### Telemetry (`crates/telemetry/`)
- [x] Counter metric type (atomic, thread-safe)
- [x] Gauge metric type (up/down)
- [x] Histogram with 11 latency buckets (1ms-10s)
- [x] MetricsSnapshot for periodic persistence
- [x] Global metrics registry (LazyLock)
- [x] Component health tracking
- [x] Health report aggregation
- [x] Tracing setup from environment

**Test:** Metrics are internal - verify via `/health` endpoint returning correct structure

---

### Configuration (`config/default.toml`, `.env.example`)
- [x] Server config (host, port, auth_url)
- [x] Redpanda config (brokers, topic, batch_size, compression)
- [x] ClickHouse config (url, database, pool_size)
- [x] Environment variable overrides

**Test:** `cargo run` with missing config should fail with clear error message

---

## Phase 2: Data Pipeline (COMPLETE)

### Redpanda Producer (`crates/redpanda/`)
- [x] rskafka client (pure Rust, no native deps)
- [x] Batch accumulator per topic
- [x] Configurable batch size and timeout
- [x] Partition key strategies (BySession, ByTenant, RoundRobin)
- [x] Compression support (gzip, snappy, lz4, zstd)
- [x] Client connection caching
- [x] Background flush task
- [x] Error tracking and metrics
- [x] Health check function
- [x] `send_clickhouse_events()` for transformed events

**Test:**
1. Start Redpanda: `docker run -p 9092:9092 vectorized/redpanda`
2. Send events via `/ingest`
3. Consume from `events` topic to verify delivery

---

### ClickHouse Schema (`crates/clickhouse/`)
- [x] Client wrapper with config
- [x] `overwatch.events` table DDL (production spec)
  - event_id, project_id, session_id, user_id
  - type (LowCardinality), timestamp (DateTime64(3))
  - url, path, referrer, user_agent
  - device_type, browser, browser_version, os (LowCardinality)
  - country, region, city
  - data (JSON blob for extensibility)
  - Partitioned by toYYYYMM(timestamp), ordered by (project_id, timestamp, event_id)
- [x] `overwatch.sessions` table DDL (ReplacingMergeTree)
- [x] `overwatch.internal_metrics` table DDL (dogfooding)
- [x] Schema initialization on startup
- [x] Event types module with all 15 types

**Test:**
1. Start ClickHouse: `docker run -p 8123:8123 clickhouse/clickhouse-server`
2. Run service, check tables created: `SELECT * FROM system.tables WHERE database='overwatch'`

---

### HTTP API (`crates/api/`)
- [x] Axum router with middleware stack
- [x] CORS, compression, tracing layers
- [x] `POST /ingest` endpoint (3 payload formats: array, object with events, single)
- [x] `GET /health` endpoint
- [x] `GET /health/ready` endpoint
- [x] `GET /health/live` endpoint
- [x] AuthContext extractor (validates API key, calls auth service)
- [x] ClientIp extractor (X-Forwarded-For aware)
- [x] Rate limiter (token bucket algorithm)
- [x] Error response formatting with spec error codes
- [x] SDK → ClickHouse event transformation

**Test:**
```bash
# Array format
curl -X POST http://localhost:8080/ingest \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer owk_live_ABC123xyz789DEF456ghi012JKL345mn" \
  -d '[{"id":"550e8400-e29b-41d4-a716-446655440000","type":"pageview","timestamp":1704067200000,"sessionId":"11111111-1111-1111-1111-111111111111","url":"https://example.com/","userAgent":"Mozilla/5.0"}]'

# Object format
curl -X POST http://localhost:8080/ingest \
  -H "Content-Type: application/json" \
  -H "X-API-Key: owk_test_ABC123xyz789DEF456ghi012JKL345mn" \
  -d '{"events":[...],"metadata":{"sdkVersion":"1.0"}}'

# Single event format
curl -X POST http://localhost:8080/ingest \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer owk_live_ABC123xyz789DEF456ghi012JKL345mn" \
  -d '{"id":"...","type":"pageview",...}'
```

**Response Format:**
```json
{"success":true,"received":42,"timestamp":1704067200000}
```

---

## Phase 3: Security & Validation (IN PROGRESS)

### API Key Authentication
- [x] Header extraction (Authorization: Bearer, X-API-Key)
- [x] API key format validation (`owk_(live|test)_[a-zA-Z0-9]{32}`)
- [x] Live/Test environment detection
- [x] Auth service request/response types
- [x] Mock auth client (returns success for valid format)
- [ ] **TODO: Real auth service HTTP client (reqwest)**
- [ ] **TODO: Auth response caching (short TTL)**
- [ ] **TODO: Rate limit per project_id from auth response**

**Test:** Should reject requests with invalid API key format

---

### Input Validation
- [x] Event schema validation (all 15 types)
- [x] Batch size limits (1-1000 events, 1MB payload)
- [x] Event size limit (64KB)
- [x] Field length constraints (URL 2048, user_agent 512, etc.)
- [x] Timestamp bounds (±24h past, 5s future)
- [x] Custom properties size limit (16KB)
- [ ] **TODO: URL/path sanitization**
- [ ] **TODO: XSS prevention in string fields**

**Test:** Send malformed events, verify 400 response with VALID_001 code

---

## Phase 4: Background Workers

### Consumer Worker (`crates/worker/consumer.rs`) - COMPLETE
- [x] Consumer struct with rskafka client
- [x] ConsumerConfig (group_id, batch_size, timeout)
- [x] fetch_batch() using PartitionClient::fetch_records()
- [x] commit() for offset management
- [x] Deserialize JSON to ClickHouseEvent
- [x] insert_clickhouse_events() for ClickHouse inserts
- [x] ConsumerWorker with retry logic
- [x] Scheduler integration (spawn consumer worker)
- [x] main.rs wiring

**Data Flow:**
```
POST /ingest → Validate → Transform → Redpanda → ConsumerWorker → ClickHouse
```

---

### Compression Worker (`crates/worker/compression.rs`)
- [x] Worker struct and scheduler integration
- [ ] **TODO: Query for free tier projects**
- [ ] **TODO: Identify data older than 24h**
- [ ] **TODO: Aggregate raw events into rollups**
- [ ] **TODO: Delete compressed raw events**
- [ ] **TODO: Parquet export for cold storage**

**Test:**
1. Insert events for free tier project
2. Wait 24h (or mock time)
3. Verify raw events aggregated and deleted

---

### Retention Worker (`crates/worker/retention.rs`)
- [x] Worker struct and scheduler integration
- [x] Tier-based retention hours calculation
- [ ] **TODO: Query for expired data per project**
- [ ] **TODO: Execute deletion queries**
- [ ] **TODO: Update deletion metrics**
- [ ] **TODO: Handle tier changes (upgrade/downgrade)**

**Test:** ClickHouse TTL handles basic retention, but tier-specific logic needs testing

---

### Enrichment Worker (`crates/worker/enrichment.rs`)
- [x] Worker struct with enable flags
- [x] Enrichment result tracking
- [ ] **TODO: GeoIP database integration (MaxMind)**
- [ ] **TODO: User agent parsing (woothee or similar)**
- [ ] **TODO: Device/browser/OS detection**
- [ ] **TODO: Bot detection**

**Test:** Enrich event with known IP, verify country/city populated

---

### Backfill Worker (`crates/worker/backfill.rs`)
- [x] Worker struct
- [x] BackfillResult type
- [ ] **TODO: Session recomputation queries**
- [ ] **TODO: Daily/hourly aggregate recomputation**
- [ ] **TODO: Progress tracking for long backfills**
- [ ] **TODO: Resumable backfill on failure**

**Test:** Trigger backfill for date range, verify aggregates updated

---

### Notification Worker (`crates/worker/notifications.rs`)
- [x] Notification types (HighErrorRate, QuotaWarning, etc.)
- [x] Channel abstraction (Log, Webhook, Email)
- [x] Alert threshold checking
- [ ] **TODO: Actual webhook HTTP calls**
- [ ] **TODO: Email sending (SMTP or service)**
- [ ] **TODO: Slack/Discord integrations**
- [ ] **TODO: Alert deduplication/cooldown**

**Test:** Trigger high error rate, verify webhook called (mock server)

---

## Phase 5: Production Hardening (NOT STARTED)

### Resilience
- [ ] Redpanda reconnection on failure
- [ ] ClickHouse connection pool recovery
- [ ] Circuit breaker pattern for downstream services
- [ ] Retry logic with exponential backoff
- [ ] Dead letter queue for failed events

### Observability
- [ ] Structured logging with request IDs
- [ ] Distributed tracing (OpenTelemetry)
- [ ] Metrics dashboard (Grafana compatible)
- [ ] Alert rules configuration

### Performance
- [ ] Connection pooling optimization
- [ ] Batch size tuning based on load
- [ ] Memory usage profiling
- [ ] Load testing (100k+ events/sec target)

### Security
- [ ] API key rotation support
- [ ] Request signing/HMAC validation
- [ ] IP allowlisting per project
- [ ] Audit logging

---

## Phase 6: Testing (PARTIAL)

### Unit Tests
- [x] SDK event parsing (3 formats)
- [x] Event transformation
- [x] API key validation
- [x] Auth response handling
- [x] Error codes
- [ ] Batch accumulator edge cases
- [ ] Rate limiter behavior
- [ ] Metric calculations

### Integration Tests
- [ ] End-to-end ingest flow
- [ ] Redpanda message verification
- [ ] ClickHouse data verification
- [ ] Health endpoint accuracy
- [ ] Auth service integration

### Load Tests
- [ ] Sustained throughput testing
- [ ] Burst handling
- [ ] Memory leak detection
- [ ] Latency percentiles (p50, p95, p99)

---

## API Spec Summary

### Authentication
```
Authorization: Bearer owk_(live|test)_[a-zA-Z0-9]{32}
X-API-Key: owk_(live|test)_[a-zA-Z0-9]{32}
```

### Error Codes
| Code | Status | Description |
|------|--------|-------------|
| AUTH_001 | 401 | API key is required |
| AUTH_002 | 401 | Invalid API key format |
| AUTH_003 | 401 | Invalid API key |
| AUTH_004 | 401 | API key has been revoked |
| AUTH_005 | 403 | Insufficient permissions |
| VALID_001 | 400 | Invalid JSON / Invalid format |
| VALID_002 | 400 | Batch exceeds 1000 events |
| VALID_003 | 400 | Event exceeds 64KB |
| DB_001 | 500 | Failed to store events |
| RATE_001 | 429 | Rate limit exceeded |

### Event Types
```
pageview, pageleave, click, scroll,
form_focus, form_blur, form_submit, form_abandon,
error, visibility_change, resource_load,
session_start, session_end, performance, custom
```

### SDK Event Schema (Input)
```json
{
  "id": "uuid",
  "type": "pageview",
  "timestamp": 1704067200000,
  "sessionId": "uuid",
  "url": "https://example.com/page",
  "userAgent": "Mozilla/5.0...",
  "userId": "optional-user-id",
  "path": "/page",
  "referrer": "https://google.com",
  "deviceInfo": {
    "device": { "type": "desktop", "os": "macOS" },
    "browser": { "name": "Chrome", "version": "120" }
  },
  "location": { "country": "US", "region": "CA", "city": "SF" },
  "customField": "extra data goes to 'data' blob"
}
```

---

## Quick Reference

### Start Development
```bash
# Start dependencies
docker-compose up -d

# Run service
cargo run

# Test ingest
curl -X POST http://localhost:8080/ingest \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer owk_live_ABC123xyz789DEF456ghi012JKL345mn" \
  -d '[{"id":"550e8400-e29b-41d4-a716-446655440000","type":"pageview","timestamp":1704067200000,"sessionId":"11111111-1111-1111-1111-111111111111","url":"https://example.com/","userAgent":"Mozilla/5.0"}]'
```

### Crate Dependencies
```
main.rs
├── api (HTTP layer)
│   ├── engine-core (types, validation, auth)
│   ├── redpanda (producer)
│   ├── clickhouse-client (storage)
│   └── telemetry (metrics)
├── worker (background tasks)
│   ├── engine-core
│   ├── clickhouse-client
│   └── telemetry
└── telemetry
```

### Event Flow
```
SDK (camelCase) → POST /ingest → Auth → Validate → Transform (snake_case) → Redpanda → ClickHouse
                                                                              ↓
                                          Worker: Compress/Retain/Enrich/Notify
```
