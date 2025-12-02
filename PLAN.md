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

## Phase 3: Security & Validation (PARTIAL)

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

## Phase 4: Background Workers (COMPLETE)

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

### Enrichment Worker (`crates/worker/enrichment.rs`) - UA PARSING COMPLETE
- [x] EnrichmentWorker struct with woothee parser
- [x] User agent parsing (woothee crate, ~6.8µs/parse)
- [x] Device type detection (desktop, mobile, bot, other)
- [x] Browser name/version extraction
- [x] OS detection
- [x] Batch enrichment support
- [x] Integration into ConsumerWorker (enrich before ClickHouse insert)
- [x] Unit tests (9 tests: Chrome, Safari, Firefox, Googlebot, edge cases)
- [ ] **TODO: GeoIP database integration (MaxMind) - skipped for now**
- [ ] **TODO: Enhanced bot detection - skipped for now**

**Test:** `cargo test -p worker enrichment` - 9 tests passing

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

## Phase 6: Testing (COMPLETE - 24 TESTS PASSING)

### Unit Tests (29 passing)
- [x] SDK event parsing (3 formats)
- [x] Event transformation
- [x] API key validation
- [x] Auth response handling
- [x] Error codes
- [x] Partitioner consistent hashing
- [x] Consumer config defaults
- [x] Enrichment UA parsing (9 tests)
- [ ] Batch accumulator edge cases
- [ ] Rate limiter behavior
- [ ] Metric calculations

### Integration Tests (24 TESTS PASSING)
Using **MockProducer** for Kafka (avoids Docker Desktop AIO limits) + real ClickHouse testcontainer:

**Architecture:**
```
POST /ingest → Real Axum Router → Real Middleware → Real Transform
    ↓
MockProducer (captures events) → process_captured_events() → Real ClickHouse
    ↓
Query ClickHouse to verify data
```

**Key Implementation:**
- `EventProducer` trait enables dependency injection
- `MockProducer` captures events in memory, implements same trait as real `Producer`
- Tests all production code paths except Kafka network transport
- ClickHouse testcontainer validates actual storage

**End-to-End Pipeline (5 tests):**
- [x] `test_ingest_array_format_e2e` - Array format → MockProducer → ClickHouse verify
- [x] `test_ingest_object_format_e2e` - Object format with metadata
- [x] `test_ingest_single_event_e2e` - Single event format
- [x] `test_ingest_mixed_event_types_e2e` - Multiple event types in batch
- [x] `test_producer_failure_returns_error` - Producer failure handling

**Error Scenarios (11 tests):**
- [x] `test_missing_api_key_returns_401` - AUTH_001
- [x] `test_invalid_api_key_format_returns_401` - AUTH_002
- [x] `test_wrong_api_key_prefix_returns_401` - AUTH_002
- [x] `test_invalid_json_returns_400` - VALID_001
- [x] `test_malformed_json_returns_400` - VALID_001
- [x] `test_batch_exceeds_limit_returns_400` - VALID_002 (1001 events)
- [x] `test_empty_batch_accepted` - Empty batch returns success
- [x] `test_event_missing_required_field_returns_400` - VALID_001
- [x] `test_invalid_event_type_returns_400` - VALID_001
- [x] `test_bearer_token_auth_works` - Authorization header
- [x] `test_both_key_environments_work` - Live and test keys

**Health Endpoints (6 tests):**
- [x] `test_health_endpoint_structure` - Response structure
- [x] `test_health_endpoint_healthy` - Status check
- [x] `test_ready_endpoint` - Readiness probe
- [x] `test_live_endpoint` - Liveness probe
- [x] `test_health_endpoints_no_auth_required` - No auth needed
- [x] `test_health_queue_depth_is_number` - Queue depth is valid number

**Mock Unit Tests (2 tests):**
- [x] `test_mock_producer_captures_events` - Event capture works
- [x] `test_mock_producer_failure_mode` - Failure simulation works

**Files:**
```
tests/
├── Cargo.toml                  # testcontainers, axum-test, async-trait deps
├── src/
│   ├── lib.rs
│   ├── containers.rs           # ClickHouse only (Redpanda mocked)
│   ├── fixtures.rs             # Event generators
│   ├── mocks.rs                # MockProducer implementation
│   └── setup.rs                # TestContext with MockProducer
└── tests/
    ├── ingest_e2e.rs           # 5 E2E pipeline tests
    ├── ingest_errors.rs        # 11 error scenario tests
    └── health.rs               # 6 health endpoint tests

crates/redpanda/src/producer.rs # EventProducer trait + impl
crates/api/src/state.rs         # Arc<dyn EventProducer>
```

**Run Tests:**
```bash
# Requires Docker for ClickHouse container only
cargo test -p integration-tests -- --test-threads=1
```

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
SDK (camelCase) → POST /ingest → Auth → Validate → Transform (snake_case) → Redpanda
                                                                              ↓
                                              ConsumerWorker: fetch → enrich (UA) → ClickHouse
                                                                              ↓
                                              Other Workers: Compress/Retain/Notify
```

---

## Code Quality

### Compiler Warnings
```bash
cargo check --workspace --exclude integration-tests  # 0 warnings
cargo clippy --workspace --exclude integration-tests # 0 warnings
```

### Test Coverage
```bash
cargo test --workspace --exclude integration-tests   # 29 unit tests passing
cargo test -p integration-tests                       # 24 integration tests (requires Docker)
```

### Dependencies
- Pure Rust where possible (rskafka, woothee)
- Minimal external dependencies
- No C bindings except zstd/lz4 for compression
