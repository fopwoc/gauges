# Gauges Console

Read-only Svelte dashboard for Gauges. It is compiled to static assets and
served by the Rust Hub from the same origin. Bun and SvelteKit have no
production runtime role.

The directory request is intentionally cheap. Summary batches cover only cards
on screen plus two cards in each direction; history batches cover only expanded
cards in that subscribed set. Off-screen telemetry and cursors are discarded.
Cards begin collapsed after each refresh, and any subset can be expanded for the
rest of that page session. Use `?view=expanded` for a non-collapsible, best-effort
wall display; data loading remains viewport-lazy.

Each history response supplies that probe's retention hours and collection
interval. The browser trims against the machine value, uses its cadence to
distinguish real chart gaps from normal spacing, and has no global
history-duration configuration.

The browser polls every five seconds by default. Pass `?poll=<seconds>` (for
example, `?poll=2`) to configure that page. Poll cadence is independent of probe
collection cadence, so a valid history delta may contain several samples or no
samples for a particular machine.

## Development

```sh
bun install
bun run dev
```

The Vite development server proxies `/api` and `/health` to
`http://127.0.0.1:8080`. Set `GAUGES_DEV_HUB_URL` to use another development
Hub.

Useful checks:

```sh
bun run check
bun test
bun run build
```

The production UI has no environment configuration, authentication, or device
actions and is intended for a trusted home network. `?poll=<seconds>` and
`?view=expanded` remain page-local browser options.
