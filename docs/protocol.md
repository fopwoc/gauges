# Gauges protocol v1

Gauges intentionally uses ordinary JSON over HTTP. There are no WebSockets,
server commands, or bidirectional control channels.

## Authentication and device ownership

The hub is configured with a map of device ids to display names and tokens.
Every ingest request sends both:

```http
X-Gauges-Device-Id: router
Authorization: Bearer replace-with-a-long-random-token
```

The hub rejects an unknown id or a token that does not match that id. Tokens
are only accepted by the write endpoint; public read endpoints intentionally
have no authentication because the console is designed for a trusted LAN.

## Ingest

`POST /api/v1/ingest` carries the current handshake identity and one or more
buffered samples:

```json
{
  "identity": {
    "hostname": "openwrt",
    "distro": "OpenWrt",
    "distroVersion": "24.10.0",
    "kernelVersion": "6.6.73",
    "ipAddress": "192.168.1.1",
    "gpuTypes": []
  },
  "retentionHours": 24,
  "collectionIntervalSeconds": 10,
  "samples": [
    {
      "sampleId": "8e26e0f7-01fe-45d3-aa5c-c01ae2224d26",
      "capturedAtMs": 1786929000000,
      "cpu": { "usagePercent": 7.3, "temperatureCelsius": 51.0 },
      "memory": {
        "totalBytes": 536870912,
        "usedBytes": 168820736,
        "availableBytes": 368050176,
        "swapTotalBytes": 0,
        "swapUsedBytes": 0
      },
      "disks": [],
      "networks": [],
      "gpus": [],
      "powerWatts": null,
      "uptimeSeconds": 184233
    }
  ]
}
```

`gpuTypes` is a stable, sorted list discovered when the probe starts. NVIDIA
entries use the NVML model name when available; DRM devices use a readable
vendor/driver type such as `AMD (amdgpu)`. Utilization, temperature, and VRAM
remain per-sample fields rather than handshake data.

`retentionHours` is the probe's positive whole-number `retention_hours`
setting. The same value limits the probe's offline outbox and tells the hub how
much history to retain for this machine. Every ingest refreshes the stored
value, so changing the probe configuration changes that machine independently.

`collectionIntervalSeconds` is the probe's positive whole-number
`collection_interval_seconds` setting. It describes expected sample spacing;
it is not the console polling interval. The hub refreshes this value on every
ingest and returns it with summary and history data.

The response acknowledges sample ids only after the central transaction has
committed:

```json
{
  "acceptedSampleIds": ["8e26e0f7-01fe-45d3-aa5c-c01ae2224d26"],
  "serverTimeMs": 1786929000123
}
```

`(device id, sample id)` is unique, so replaying a batch after an ambiguous
network failure is safe. The probe deletes only acknowledged local rows.

## Time

Each sample contains the probe's Unix timestamp. The hub stores that timestamp
unchanged and separately records its own receipt time. It does not estimate or
correct clock skew. Each machine's retention is evaluated against hub receipt
time using its probe-supplied hour count, while charts and online state use the
original capture time.

## Read API and polling

- `GET /health` reports hub liveness.
- `GET /api/v1/devices/index` returns the sorted configured device ids, their
  total/online/offline counts, newest probe-local metric timestamp, and a
  revision fingerprint. It reads only compact device status metadata, not
  retained samples, so the console can draw the complete card directory and
  fleet counters cheaply.
- `POST /api/v1/devices/sync` accepts `{"deviceIds":["router","nas"]}` and
  returns a map from each requested configured id to its current summary. The
  value is `null` until that device completes its first authenticated ingest.
- `POST /api/v1/history/sync` accepts a `machines` map. Each key is a device id;
  each value is either `null` for the full retained history or the last
  probe-local metric timestamp already known by the console.

For example, an incremental history request can mix first loads and deltas:

```json
{
  "machines": {
    "router": 1786929000000,
    "nas": null
  }
}
```

The response preserves the same map shape. Each machine contains its
`retentionHours`, `collectionIntervalSeconds`, ascending samples, and
`nextMetricTimeMs`, the greatest probe `capturedAtMs` returned (or the supplied
timestamp when no newer row exists).

The history timestamp boundary is exclusive: only samples with a greater probe
timestamp are returned. With this deliberately simple timestamp cursor, two
samples from one probe captured in the same millisecond are not distinguishable
after the first sync, and a clock moving backward can hide updates while a card
remains subscribed. Evicting and later resubscribing the card performs a fresh
full sync. Probe clocks are not normalized or aligned between machines.

Both batch endpoints accept up to the current hard limit of 1,000 machines.
Unknown ids are ignored. The console requests summaries only for the
viewport plus its overscan window, requests histories only for expanded members
of that set, and discards both data and cursors after a machine leaves it.
Console polling is independent of probe collection. One machine may return
several new samples in a delta while another returns none; an empty delta does
not mean that the machine is offline and does not create a chart gap. Liveness
compares the latest probe timestamp with current time using a grace of at least
twice the machine's advertised collection interval. Chart gaps use the
advertised interval against adjacent probe timestamps. Neither calculation
attempts clock-skew correction.
There is no separate hub or console history-duration cap. A very large probe
retention value therefore produces a proportionally large database and initial
history response; selecting a practical number of hours is the operator's
responsibility.

Unknown JSON fields are ignored for forward compatibility within protocol v1.
The embedded redb schemas are deliberately destructive: a schema version
change drops old metrics rather than migrating them.
