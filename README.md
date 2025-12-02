# Overwatch Analytics Ingestion Engine

High-throughput event ingestion pipeline for real-time analytics.

```
SDK Events → Validation → Redpanda → ClickHouse → Dashboards
```

## Architecture

The engine processes analytics events from client SDKs through a durable, scalable pipeline:

- **Event Types**: pageview, click, scroll, performance, custom
- **Throughput Target**: 100k+ events/sec
- **Durability**: Redpanda message broker with configurable batching
- **Storage**: ClickHouse columnar database with materialized views
- **Retention**: Tiered (Free: 24h, Paid: 90d, Enterprise: custom)

## Project Structure

```
ingestion-engine/
├── src/main.rs              # Application entry point
├── crates/
│   ├── core/                # Event types, validation, schemas
│   ├── api/                 # HTTP API (Axum)
│   ├── redpanda/            # Kafka-compatible producer
│   ├── clickhouse/          # ClickHouse client & inserts
│   ├── worker/              # Background tasks
│   └── telemetry/           # Internal metrics & tracing
└── config/
    └── default.toml         # Default configuration
```

## Quick Start

### Prerequisites

- Rust 1.75+
- Docker (for Redpanda and ClickHouse)

### Setup

```bash
# Start dependencies
docker run -d --name redpanda -p 9092:9092 vectorized/redpanda
docker run -d --name clickhouse -p 8123:8123 clickhouse/clickhouse-server

# Copy environment config
cp .env.example .env

# Run the service
cargo run
```

### Test Ingestion

```bash
curl -X POST http://localhost:8080/ingest \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer 00000000-0000-0000-0000-000000000000:secret" \
  -d '{
    "events": [{
      "id": "550e8400-e29b-41d4-a716-446655440000",
      "tenant_id": "00000000-0000-0000-0000-000000000000",
      "session_id": "11111111-1111-1111-1111-111111111111",
      "timestamp": "2024-01-01T00:00:00Z",
      "type": "pageview",
      "title": "Home",
      "path": "/"
    }]
  }'
```

### Health Check

```bash
# Full health status
curl http://localhost:8080/health

# Kubernetes probes
curl http://localhost:8080/health/ready
curl http://localhost:8080/health/live
```

## Configuration

Configuration is loaded from multiple sources (in order of precedence):

1. Environment variables (prefix: `INGESTION_`)
2. `config/default.toml`
3. Built-in defaults

### Key Settings

| Variable | Default | Description |
|----------|---------|-------------|
| `INGESTION_HOST` | `0.0.0.0` | Server bind address |
| `INGESTION_PORT` | `8080` | Server port |
| `INGESTION_REDPANDA_BROKERS` | `localhost:9092` | Redpanda broker list |
| `INGESTION_REDPANDA_BATCH_SIZE` | `1000` | Max events per batch |
| `INGESTION_CLICKHOUSE_URL` | `http://localhost:8123` | ClickHouse HTTP URL |
| `INGESTION_CLICKHOUSE_DATABASE` | `overwatch` | Database name |

## API Reference

### POST /ingest

Accepts event batches from SDK clients.

**Headers:**
- `Authorization: Bearer <tenant_id>:<api_key>` (required)
- `Content-Type: application/json`

**Request Body:**
```json
{
  "events": [
    {
      "id": "uuid",
      "tenant_id": "uuid",
      "session_id": "uuid",
      "timestamp": "ISO8601",
      "type": "pageview|click|scroll|performance|custom",
      ...event-specific fields
    }
  ]
}
```

**Response:**
```json
{
  "status": "success",
  "batch_id": "uuid",
  "events_accepted": 100,
  "events_rejected": 0,
  "ingest_latency_ms": 15
}
```

### GET /health

Returns service health status including Redpanda and ClickHouse connectivity.

## Development

```bash
# Run tests
cargo test

# Run with debug logging
RUST_LOG=debug cargo run

# Check formatting
cargo fmt --check

# Run clippy
cargo clippy
```

## Crate Dependencies

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

## License

MIT
