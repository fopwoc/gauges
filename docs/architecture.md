# Architecture

```text
Linux host                         Gauges container
┌──────────────────────────┐       ┌──────────────────────────────┐
│ gauges-probe             │ HTTP  │ Rust hub :8080               │
│                          ├──────►│                              │
│ sysinfo + Linux sysfs    │ batch │ Axum + token map + redb     │
│ redb outbox              │ retry │ API + static Console        │
└──────────────────────────┘       └──────────────┬───────────────┘
                                                 │ same-origin HTTP
                                  ┌──────────────▼───────────────┐
                                  │ browser                      │
                                  │ Svelte + UnoCSS + uPlot      │
                                  │ no auth and no actions       │
                                  └──────────────────────────────┘
```

The redb database in the probe is an outbox, not a second source of truth. A
sample is inserted locally before network I/O and removed only after the hub
acknowledges it. The hub uses the sample id for idempotency. This yields
at-least-once delivery with one stored sample per id without a broker.

At the default 10-second cadence, one device produces 8,640 samples per day.
Ten devices produce 86,400 samples, which is intentionally within the range
where ordered embedded key-value storage stays simple and predictable.

Hub and Probe share their Serde protocol models but own separate storage
semantics. Probe orders its outbox by capture time. Hub atomically maintains
sample-id, capture-time, and receipt-time indexes: sample id makes replay
idempotent, capture time serves history, and receipt time drives retention.
Values remain JSON documents so the persisted representation follows the same
shared models as the network protocol.

The Console is compiled during the multi-stage image build. Bun is not present
at runtime; Axum serves the static assets and JSON routes from one process and
one port. Local frontend development uses Vite's development proxy, while the
production browser always uses same-origin `/api/v1` requests.

Retention has one owner: each probe's positive integer `retention_hours`
setting, which defaults to 24. The probe applies it to its offline outbox and
sends it with every ingest. The hub persists the value on that device, applies
it to receipt-time cleanup and reads, and returns it with history. The console
does not impose another duration limit. Different machines can therefore keep
one hour, 24 hours, seven weeks expressed in hours, or any other operator-chosen
range without changing hub or console configuration. Storage, memory, and
initial-response costs scale with that choice.

Sampling cadence also has one owner: each probe's positive integer
`collection_interval_seconds`. The probe sends it with every ingest, the hub
persists it per machine, and the console uses it to interpret spacing between
probe timestamps. The console's HTTP polling cadence is independent and
defaults to five seconds, with `?poll=<seconds>` as a page-local override. A
single delta batch may therefore contain multiple samples for one machine and
none for another without implying missing telemetry.

Linux is a product constraint, not an abstraction leak to hide. The probe uses
portable Rust libraries for ordinary system counters and small, explicit
sysfs readers for Linux-only facts those libraries do not expose: physical NIC
classification, DRM GPU counters, and RAPL energy. No macOS or Windows fallback
is compiled or promised.

## Console synchronization

The console uses lazy synchronization rather than keeping a browser-side copy
of the whole fleet:

1. The device index supplies all ids, online/offline counts, and the newest
   probe-local metric timestamp, allowing card shells and fleet status to be
   laid out before full summaries are loaded.
2. An intersection observer reports cards actually on screen. The subscription
   set expands that range by two cards in each direction.
3. One summary batch refreshes the subscribed cards. Summary state is evicted as
   soon as a card leaves the subscription set.
4. Any subset of subscribed cards may be expanded. One history batch maps each
   expanded id to its nullable last probe metric time; `null` requests the full
   retained window and a timestamp requests samples strictly newer than it.
5. Returned machine retention hours define browser trimming, while returned
   collection intervals define chart-gap detection; there is no global console
   history window or assumed probe speed.
6. History samples and cursors are also evicted off screen. Expansion intent is
   retained only for the current page session, while a refresh starts collapsed.

Polling pauses while the document is hidden and resumes immediately when it
becomes visible. Loading shells keep the final card dimensions stable and fade
into live state after the first batch completes.

Summary polling determines neither liveness nor sample gaps by whether a delta
contains rows. The hub derives liveness from the latest probe-local metric time
with a grace of at least twice that probe's collection interval. The console
derives gaps only from probe-local sample timestamps. Neither layer estimates
or corrects clock skew.

The dashboard is a centered responsive list with a maximum of two equal-width
columns and a maximum card width. Expanded cards remain in their column rather
than becoming full-width or using masonry. `?view=expanded` forces every card
open and removes the accordion controls; viewport and history loading remain
lazy, making the same page suitable for a best-effort wall display.

Compact cards show current CPU, RAM, optional GPU, storage, network, swap, and
power state. Online status and uptime share one status line. Storage is the
aggregate of the logical filesystems visible to the probe, with the fullest
mount called out; expanded history remains per mount/device. The history charts
use probe-local time and deliberately do not imply cross-machine clock alignment.
