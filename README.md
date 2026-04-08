# ipecho

A lightweight service that returns client connection metadata as pretty-printed JSON, similar to [ifconfig.me](https://ifconfig.me). Identifies cloud provider and region by matching the client IP against AWS, GCP, and Oracle IP ranges synced every 12 hours.

## Response

```json
{
  "ip": "203.0.113.1",
  "user_agent": "curl/8.7.1",
  "host": "echo.example.com",
  "headers": {
    "accept": "*/*",
    "host": "echo.example.com",
    "user-agent": "curl/8.7.1"
  },
  "cloud_provider": "aws",
  "region": "us-east-1",
  "service": "AMAZON"
}
```

If the client IP doesn't match any known cloud provider range, `cloud_provider`, `region`, and `service` will be `null`.

## Quick Start

### Docker (pre-built image)

```bash
docker run -p 8083:8083 ghcr.io/adeekshith/echo:latest
curl http://localhost:8083
```

### Docker Compose (pre-built image)

```bash
cp .env.example .env    # edit as needed
docker compose up -d
curl http://localhost:8083
```

### Docker Compose with custom configuration

```bash
docker compose up -d -e PORT=9090 -e LOG_LEVEL=debug
```

Or override environment variables in `.env`:

```bash
PORT=9090
LOG_LEVEL=debug
RATE_LIMIT_PER_SECOND=50
```

## Building from Source

### Docker

```bash
docker build -t ipecho .
docker run -p 8083:8083 ipecho
```

### Docker Compose (build from source)

```bash
docker compose -f docker-compose.build.yml up -d
```

### Run Tests

```bash
# Run all tests (unit + integration + e2e) inside Docker
docker build -f Dockerfile.test -t ipecho-test .
docker run --rm ipecho-test
```

## Endpoints

| Endpoint | Description |
|----------|-------------|
| `GET /` | Client info as pretty-printed JSON |
| `GET /health` | Per-provider sync status, total CIDRs, degraded/ok |
| `GET /metrics` | Prometheus metrics (text format) |

## Configuration

All configuration is via environment variables:

| Variable | Default | Description |
|----------|---------|-------------|
| `PORT` | `8083` | Listen port |
| `LOG_LEVEL` | `info` | Tracing log level |
| `SYNC_INTERVAL_SECS` | `43200` | IP range sync interval (12h) |
| `TRUSTED_PROXIES` | `127.0.0.1/32,...` | CIDRs to trust XFF/X-Real-IP from |
| `RATE_LIMIT_PER_SECOND` | `10` | Requests per IP per second |
| `RATE_LIMIT_BURST` | `20` | Burst capacity per IP |

## Architecture

- **Rust / Axum** - async HTTP framework
- **In-memory CIDR lookup** - ~15k IP ranges loaded into a sorted `Vec`, sub-millisecond linear scan with longest-prefix match
- **Concurrent sync** - fetches AWS, GCP, Oracle ranges in parallel every 12h, atomically swaps the lookup table
- **Per-IP rate limiting** - token-bucket rate limiter using the `governor` crate
- **IPv4-in-IPv6 normalization** - `::ffff:x.x.x.x` addresses are mapped to IPv4 before lookup

### Adding a new IP range provider

1. Create `src/providers/your_provider.rs` implementing the `IpRangeProvider` trait
2. Register it in `src/sync/scheduler.rs` in the providers list

No other changes required.

## IP Range Sources

| Provider | Source URL |
|----------|-----------|
| AWS | https://ip-ranges.amazonaws.com/ip-ranges.json |
| GCP | https://www.gstatic.com/ipranges/cloud.json |
| Oracle | https://docs.oracle.com/en-us/iaas/tools/public_ip_ranges.json |

## Metrics

Prometheus metrics available at `/metrics`:

- `http_requests_total` - request counter by endpoint
- `ip_lookup_total` - lookup results (hit/miss)
- `sync_total` - sync results per provider (success/error)
- `sync_cidr_count` - current CIDR count per provider
- `rate_limit_rejected_total` - rate-limited requests
