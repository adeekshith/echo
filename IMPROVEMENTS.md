# Improvements Plan

## Critical Issues

### 1. Fix Dockerfile deduplication bug
**File:** `Dockerfile:17-18`

The Dockerfile has redundant `rm` patterns for `ipecho*`:
```dockerfile
rm -rf target/release/ipecho* \
       target/release/ipecho*
```

**Action:** Remove the duplicate line.

---

### 2. Pin dependency major versions
**File:** `Cargo.toml`

Most dependencies use single-version specifiers which can cause unexpected breaking changes.

**Current:**
```toml
axum = "0.8"
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
```

**Recommended:**
```toml
axum = "0.8.1"
tokio = { version = "1.38", features = ["full"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
reqwest = { version = "0.12", features = ["json", "rustls-tls"], default-features = false }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
metrics = "0.24"
metrics-exporter-prometheus = "0.16"
governor = "0.8"
```

---

## Architecture Improvements

### 3. Switch to radix tree for IP lookups
**File:** `src/lookup/mod.rs`

**Current:** Linear scan on ~15k ranges (O(n) complexity)

**Recommended:** Use `ipnetwork::IpNetworkTree` for O(log n) lookups.

**Benefits:**
- Sub-millisecond lookups become microseconds
- Better performance under load
- More scalable as IP range count grows

---

### 4. Add config validation at startup
**File:** `src/config.rs`, `src/main.rs`

**Current:** Environment variables parsed but not validated.

**Recommended:** Add validation checks:
- `PORT` is within valid range (1-65535)
- `RATE_LIMIT_PER_SECOND` > 0
- `SYNC_INTERVAL_SECS` > 0
- `EXCLUDED_HEADERS` are valid header names

**Action:** Implement `Config::validate()` method called in `main()`.

---

### 5. Enhance health endpoint
**File:** `src/handlers/health.rs`

**Current:** Returns "ok" or "degraded" based on lookup table emptiness.

**Recommended:** Add readiness checks:
- Lookup table loaded (current)
- All providers healthy (no recent sync errors)
- Server ready to accept traffic

**Response:**
```json
{
  "status": "healthy",
  "ready": true,
  "providers": [...]
}
```

---

## Code Quality

### 6. Standardize error handling with thiserror ✅
**Files:** `src/errors.rs` (new), all handler files

**Current:**
- `anyhow` used in providers and main
- `Result<_, StatusCode>` used in handlers
- `thiserror` is declared but unused

**Completed:**
1. ✅ Created `src/errors.rs` with `AppError` enum using `thiserror`
2. ✅ Handlers return `Result<_, AppError>`
3. ✅ `AppError` implements `IntoResponse` for automatic HTTP response conversion

**Implementation:**
```rust
// src/errors.rs
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("JSON serialization failed")]
    JsonError(#[from] serde_json::Error),
    
    #[error("HTTP builder failed")]
    HttpBuilderError,
    
    #[error("Header parsing failed")]
    HeaderError(#[from] axum::http::header::ToStrError),
    
    #[error("Provider sync failed: {0}")]
    ProviderError(String),
    
    #[error("Not found: {0}")]
    NotFound(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        // Returns JSON error with appropriate status code
    }
}
```

---

### 7. Add latency histograms to metrics
**File:** `src/handlers/echo.rs`, `src/sync/scheduler.rs`

**Current:** Only counters (`http_requests_total`, `ip_lookup_total`, `sync_total`)

**Recommended:** Add histogram metrics:
- `ip_lookup_duration_seconds` - IP lookup latency
- `sync_duration_seconds` - Sync operation latency
- `http_request_duration_seconds` - Request handling latency

---

## Security & Operations

### 8. Add graceful shutdown handler
**File:** `src/main.rs`

**Current:** No explicit shutdown handling.

**Recommended:** Add SIGTERM handler:
```rust
tokio::select! {
    _ = shutdown_signal() => {
        tracing::info!("shutdown signal received");
    }
    _ = server => {}
}
```

**Benefits:** Clean container shutdown, proper logging on exit.

---

### 9. Add request ID correlation to logs
**File:** `src/routes.rs` or add middleware

**Current:** Logs lack request correlation IDs.

**Recommended:** Add request ID to each request:
- Generate UUID per request
- Include in all logs for that request
- Return in response headers (`X-Request-ID`)

**Benefits:** Easier debugging, trace request through logs.

---

## Priority

1. **P0 (Critical):** Fix Dockerfile deduplication bug
2. **P1 (High):** Standardize error handling with thiserror ✅ **COMPLETED**
3. **P1 (High):** Add integration tests for error handling ✅ **COMPLETED**
4. **P1 (High):** Add integration tests for rate limiting ✅ **COMPLETED**
5. **P1 (High):** Add config validation at startup
6. **P2 (Medium):** Add graceful shutdown handler
7. **P2 (Medium):** Add request ID correlation
8. **P3 (Low):** Switch to radix tree for lookups
9. **P3 (Low):** Add latency histograms
10. **P3 (Low):** Pin dependency versions
11. **P3 (Low):** Enhance health endpoint
