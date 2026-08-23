# Gauges hub

Small Rust/Axum collector for Gauges probes. It accepts authenticated batches,
keeps retained history in redb, exposes read-only polling endpoints, and serves
the compiled Svelte Console from the same process.

## Configuration

The hub is configured only through environment variables:

| Variable | Default | Meaning |
| --- | --- | --- |
| `GAUGES_DEVICES_JSON` | required | JSON array of `{\"id\",\"name\",\"token\"}` objects |
| `GAUGES_DATABASE_PATH` | `/data/gauges.redb` | redb database file |
| `GAUGES_CONSOLE_PATH` | `/opt/gauges/console` | Compiled Console directory |
| `GAUGES_HOST` | `0.0.0.0` | Bind address |
| `GAUGES_PORT` | `8080` | HTTP port |
| `GAUGES_CLEANUP_INTERVAL_SECONDS` | `60` | Cleanup cadence |
| `GAUGES_ONLINE_THRESHOLD_SECONDS` | `30` | Minimum grace since the last upload before a device can be offline |
| `GAUGES_MAX_INGEST_BATCH_SIZE` | `512` | Maximum samples per upload |

Example: `GAUGES_DEVICES_JSON='[{\"id\":\"router\",\"name\":\"Router\",\"token\":\"change-me\"}]'`.
Device IDs may contain only letters, digits, periods, underscores, and hyphens.

## HTTP API

- `GET /health`
- `POST /api/v1/ingest` with `X-Gauges-Device-Id` and
  `Authorization: Bearer <token>`
- `GET /api/v1/devices/index`
- `POST /api/v1/devices/sync` with `{"deviceIds":["router","nas"]}`
- `POST /api/v1/history/sync` with
  `{"machines":{"router":<last-probe-metric-time-or-null>}}`

Each ingest batch includes the current device identity, its detected GPU types,
the probe's positive whole-number `retentionHours`, and its positive whole-number
`collectionIntervalSeconds`. Re-sending the same `sampleId` for the same device
is safe. Client capture time is retained exactly. Probe metric time drives
online/offline state and chart timestamps; hub receipt time drives retention.
Clock skew is not estimated or corrected. A device remains online for at least
twice its advertised collection interval, even when several console polls
contain no new samples. The hub has no global retention-hours setting.

History responses include `retentionHours`, `collectionIntervalSeconds`, and
`nextMetricTimeMs`. Send `null` for the initial retained window, then send that
probe-local timestamp on polls to receive only samples with a greater probe
timestamp. Read batches accept at most 1,000 machine ids. An empty incremental
result is normal and says nothing about liveness.

The redb schema has an integer version. A version mismatch drops all Hub tables
and recreates them; there are deliberately no data migrations. A legacy SQLite
file at the configured path is replaced on startup.
