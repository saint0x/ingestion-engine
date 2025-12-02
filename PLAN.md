# Overwatch Ingestion Engine - Production Roadmap

High-throughput analytics ingestion pipeline: SDK events → Validation → Redpanda → ClickHouse

---

## Phase 1: Foundation (COMPLETE)

### Core Types & Validation (`crates/core/`)
- [x] Event type definitions (pageview, click, scroll, performance, custom)
- [x] Event metadata schema (user_agent, ip, screen, viewport, timezone, language)
- [x] Validation rules with `validator` crate
- [x] Unified error types with descriptive messages
- [x] Session management with 30-min timeout
- [x] Tenant/API key data structures
- [x] Retention tier system (Free/Paid/Enterprise)
- [x] Retention policy configuration

**Test:** `cargo test -p engine-core` - validates event serialization and schema validation

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
- [x] Server config (host, port)
- [x] Redpanda config (brokers, batch_size, compression)
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

**Test:**
1. Start Redpanda: `docker run -p 9092:9092 vectorized/redpanda`
2. Send events via `/ingest`
3. Consume from `events_*` topics to verify delivery

---

### ClickHouse Schema (`crates/clickhouse/`)
- [x] Client wrapper with config
- [x] `events` table DDL (all event types flattened)
- [x] `sessions` table DDL (aggregates)
- [x] `internal_metrics` table DDL (dogfooding)
- [x] EventRow conversion from Event
- [x] Batch insert function
- [x] Metrics insert function
- [x] Health check function
- [x] Schema initialization on startup

**Test:**
1. Start ClickHouse: `docker run -p 8123:8123 clickhouse/clickhouse-server`
2. Run service, check tables created: `SELECT * FROM system.tables WHERE database='overwatch'`

---

### HTTP API (`crates/api/`)
- [x] Axum router with middleware stack
- [x] CORS, compression, tracing layers
- [x] `POST /ingest` endpoint
- [x] `GET /health` endpoint
- [x] `GET /health/ready` endpoint
- [x] `GET /health/live` endpoint
- [x] TenantId extractor (from header or API key)
- [x] ApiKey extractor
- [x] ClientIp extractor (X-Forwarded-For aware)
- [x] Rate limiter (token bucket algorithm)
- [x] Error response formatting
- [x] Event enrichment (IP, timestamp, tenant enforcement)

**Test:**
```bash
curl -X POST http://localhost:8080/ingest \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer <tenant_id>:secret" \
  -d '{"events":[...]}'
```

---

## Phase 3: Security & Validation (PARTIAL)

### API Key Authentication
- [x] Header extraction (Authorization, X-API-Key)
- [x] Tenant ID parsing from API key format
- [ ] **TODO: API key secret validation against database**
- [ ] **TODO: API key hash comparison**
- [ ] **TODO: API key expiration check**
- [ ] **TODO: Rate limit per tenant (currently global only)**

**Test:** Should reject requests with invalid API keys (currently accepts any)

---

### Input Validation
- [x] Event schema validation
- [x] Batch size limits (1-1000 events)
- [x] Field length constraints
- [x] Numeric range validation
- [ ] **TODO: Custom event property size limits**
- [ ] **TODO: Request body size limits**

**Test:** Send malformed events, verify 422 response with validation errors

---

## Phase 4: Background Workers (SCAFFOLDED - NEEDS IMPLEMENTATION)

### Compression Worker (`crates/worker/compression.rs`)
- [x] Worker struct and scheduler integration
- [ ] **TODO: Query for free tier tenants**
- [ ] **TODO: Identify data older than 24h**
- [ ] **TODO: Aggregate raw events into rollups**
- [ ] **TODO: Delete compressed raw events**
- [ ] **TODO: Parquet export for cold storage**

**Test:**
1. Insert events for free tier tenant
2. Wait 24h (or mock time)
3. Verify raw events aggregated and deleted

---

### Retention Worker (`crates/worker/retention.rs`)
- [x] Worker struct and scheduler integration
- [x] Tier-based retention hours calculation
- [ ] **TODO: Query for expired data per tenant**
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
- [ ] IP allowlisting per tenant
- [ ] Audit logging

---

## Phase 6: Testing (NOT STARTED)

### Unit Tests
- [ ] Core validation edge cases
- [ ] Batch accumulator logic
- [ ] Rate limiter behavior
- [ ] Metric calculations

### Integration Tests
- [ ] End-to-end ingest flow
- [ ] Redpanda message verification
- [ ] ClickHouse data verification
- [ ] Health endpoint accuracy

### Load Tests
- [ ] Sustained throughput testing
- [ ] Burst handling
- [ ] Memory leak detection
- [ ] Latency percentiles (p50, p95, p99)

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
  -H "Authorization: Bearer 00000000-0000-0000-0000-000000000000:secret" \
  -d '{"events":[{"id":"550e8400-e29b-41d4-a716-446655440000","tenant_id":"00000000-0000-0000-0000-000000000000","session_id":"11111111-1111-1111-1111-111111111111","timestamp":"2024-01-01T00:00:00Z","type":"pageview","title":"Home","path":"/"}]}'
```

### Crate Dependencies
```
main.rs
├── api (HTTP layer)
│   ├── engine-core (types, validation)
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
SDK → POST /ingest → Validate → Enrich → Batch → Redpanda → ClickHouse MV
                                              ↓
                              Worker: Compress/Retain/Enrich/Notify
```
