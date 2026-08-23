<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import { Tween, prefersReducedMotion } from 'svelte/motion';
  import DeviceCard from '$lib/components/DeviceCard.svelte';
  import MachineCardSkeleton from '$lib/components/MachineCardSkeleton.svelte';
  import { FleetSync } from '$lib/fleet/FleetSync.svelte';

  const query = new URLSearchParams(window.location.search);
  const forceExpanded = query.get('view') === 'expanded';
  const requestedPollSeconds = Number(query.get('poll'));
  const pollIntervalMs = (
    Number.isFinite(requestedPollSeconds) && requestedPollSeconds > 0
      ? requestedPollSeconds
      : 5
  ) * 1_000;

  const fleet = untrack(() => new FleetSync({
    forceExpanded,
    pollIntervalMs
  }));
  const observedNodes = new Map<Element, string>();
  let intersectionObserver: IntersectionObserver | undefined;
  let visibleIds = $state<Set<string>>(new Set());
  const animatedOnline = new Tween(0);
  const animatedOffline = new Tween(0);
  const clockFormatter = new Intl.DateTimeFormat(undefined, {
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
    hourCycle: 'h23'
  });
  let browserNowMs = $state(Date.now());

  $effect(() => {
    void animatedOnline.set(fleet.online, {
      duration: prefersReducedMotion.current ? 0 : 320
    });
    void animatedOffline.set(fleet.offline, {
      duration: prefersReducedMotion.current ? 0 : 320
    });
  });

  let hubTimeMs = $derived(fleet.serverTimeObservedAtMs == null
    ? null
    : fleet.serverTimeMs + Math.max(0, browserNowMs - fleet.serverTimeObservedAtMs)
  );
  let localMetricTime = $derived(
    fleet.lastMetricTimeMs == null ? '' : clockFormatter.format(fleet.lastMetricTimeMs)
  );

  function observeMachine(node: Element, deviceId: string) {
    observedNodes.set(node, deviceId);
    intersectionObserver?.observe(node);
    return {
      update(nextDeviceId: string) {
        observedNodes.set(node, nextDeviceId);
      },
      destroy() {
        const removedId = observedNodes.get(node);
        intersectionObserver?.unobserve(node);
        observedNodes.delete(node);
        if (!removedId || !visibleIds.has(removedId)) return;
        const next = new Set(visibleIds);
        next.delete(removedId);
        visibleIds = next;
        fleet.setVisibleDeviceIds(next);
      }
    };
  }

  onMount(() => {
    intersectionObserver = new IntersectionObserver((entries) => {
      const next = new Set(visibleIds);
      for (const entry of entries) {
        const deviceId = observedNodes.get(entry.target);
        if (!deviceId) continue;
        if (entry.isIntersecting) next.add(deviceId);
        else next.delete(deviceId);
      }
      visibleIds = next;
      fleet.setVisibleDeviceIds(next);
    });
    for (const node of observedNodes.keys()) intersectionObserver.observe(node);
    const stop = fleet.start();
    const clockTimer = window.setInterval(() => {
      browserNowMs = Date.now();
    }, 250);
    return () => {
      stop();
      window.clearInterval(clockTimer);
      intersectionObserver?.disconnect();
      observedNodes.clear();
    };
  });
</script>

<svelte:head>
  <title>Gauges · Home lab</title>
</svelte:head>

<div class="shell">
  <header class="topbar">
    <a class="brand" href="/" aria-label="Gauges dashboard">
      <img src="/favicon.svg" width="28" height="28" alt="" />
      <strong>Gauges</strong>
    </a>
    <div class="header-times">
      <time datetime={hubTimeMs == null ? undefined : new Date(hubTimeMs).toISOString()}>{hubTimeMs == null ? '' : `Hub ${clockFormatter.format(hubTimeMs)}`}</time>
      <time
        class:disconnected={Boolean(fleet.hubError)}
        datetime={fleet.lastMetricTimeMs == null ? undefined : new Date(fleet.lastMetricTimeMs).toISOString()}
        aria-label={fleet.hubError && localMetricTime ? `Hub unavailable. Last metric ${localMetricTime}` : undefined}
      >{localMetricTime ? `Last metric ${localMetricTime}` : ''}</time>
    </div>
  </header>

  <main>
    <section class="fleet-head" aria-labelledby="fleet-title">
      <h1 id="fleet-title">Machines</h1>
      <dl class="fleet-counts">
        <div><dt>Online</dt><dd>{Math.round(animatedOnline.current)}</dd></div>
        <div><dt>Offline</dt><dd>{Math.round(animatedOffline.current)}</dd></div>
      </dl>
    </section>

    {#if forceExpanded}
      <div class="view-policy" role="status">
        All machines are expanded by the <code>view=expanded</code> URL policy. Off-screen telemetry remains lazy.
      </div>
    {/if}

    <section class="machine-index" aria-label="Monitored machines">
      {#if fleet.directoryLoading && fleet.deviceIds.length === 0}
        <div class="machine-grid" aria-label="Loading machine directory" aria-busy="true">
          <div class="machine-slot"><MachineCardSkeleton active /></div>
          <div class="machine-slot"><MachineCardSkeleton active /></div>
        </div>
      {:else if fleet.deviceIds.length === 0}
        <div class="empty-state">
          <img src="/favicon.svg" width="48" height="48" alt="" />
          <h2>No configured machines</h2>
          <p>Add a machine to the hub configuration and it will receive a card before its first upload.</p>
        </div>
      {:else}
        <div class="machine-grid">
          {#each fleet.deviceIds as deviceId (deviceId)}
            {@const summary = fleet.summaryState(deviceId)}
            {@const expanded = fleet.isExpanded(deviceId)}
            <div class="machine-slot" use:observeMachine={deviceId} data-device-id={deviceId}>
              {#if summary.initialized && summary.device}
                <DeviceCard
                  device={summary.device}
                  history={fleet.historyState(deviceId)}
                  serverTimeMs={fleet.serverTimeMs}
                  isOpen={expanded}
                  {forceExpanded}
                  onToggle={(id, open) => fleet.setExpanded(id, open)}
                />
              {:else if summary.initialized}
                <article class="unreported">
                  <div><strong>{deviceId}</strong><span>Configured machine</span></div>
                  <p>Awaiting its first authenticated probe upload.</p>
                </article>
              {:else if summary.error}
                <article class="unreported error">
                  <div><strong>{deviceId}</strong><span>Summary unavailable</span></div>
                  <p>{summary.error}</p>
                </article>
              {:else}
                <MachineCardSkeleton active={fleet.isSubscribed(deviceId)} expanded={expanded} />
              {/if}
            </div>
          {/each}
        </div>
      {/if}
    </section>
  </main>
</div>

<style>
  /* Hallmark · pre-emit critique: P5 H5 E5 S5 R5 V5
   * genre: modern-minimal · macrostructure: Workbench · design-system: Cobalt
   * designed-as-app · columns: centred responsive 1–2 · enrichment: none
   */
  .shell { display: flex; width: min(var(--page-width), 100%); min-height: 100vh; min-height: 100dvh; flex-direction: column; margin: 0 auto; padding-inline: var(--page-gutter); }
  .topbar { display: flex; min-height: var(--space-2xl); align-items: center; justify-content: space-between; border-bottom: var(--rule-hairline) solid var(--color-rule); }
  .brand { display: flex; align-items: center; gap: var(--space-sm); color: var(--color-ink); text-decoration: none; }
  .brand:active { color: var(--color-accent); }
  .brand strong { font-family: var(--font-display); font-size: var(--text-sm); font-weight: 680; letter-spacing: -0.02em; }
  .brand img { display: block; flex: 0 0 auto; }
  .header-times { display: flex; min-width: 10.75rem; flex-direction: column; align-items: flex-end; gap: var(--space-3xs); }
  .header-times time { min-height: 1em; color: var(--color-muted); font-family: var(--font-mono); font-size: var(--text-xs); font-variant-numeric: tabular-nums; text-align: right; white-space: nowrap; }
  .header-times time.disconnected { color: var(--color-danger); }
  main { flex: 1 0 auto; padding-top: var(--space-xl); }
  .fleet-head { display: flex; align-items: end; justify-content: space-between; gap: var(--space-xl); padding-bottom: var(--space-md); }
  .fleet-head h1 { min-width: 0; margin: 0; color: var(--color-ink); font-family: var(--font-display); font-size: var(--text-xl); font-weight: 660; letter-spacing: -0.045em; line-height: 1; overflow-wrap: anywhere; }
  .fleet-counts { display: flex; gap: var(--space-lg); margin: 0; }
  .fleet-counts > div { display: flex; min-width: 4rem; flex-direction: column-reverse; gap: var(--space-3xs); text-align: right; }
  .fleet-counts dt { color: var(--color-muted); font-family: var(--font-mono); font-size: var(--text-xs); white-space: nowrap; }
  .fleet-counts dd { margin: 0; color: var(--color-ink); font-family: var(--font-display); font-size: var(--text-md); font-variant-numeric: tabular-nums; font-weight: 620; }
  .view-policy { margin-bottom: var(--space-sm); padding: var(--space-sm) var(--space-md); border: var(--rule-hairline) solid var(--color-rule); border-radius: var(--radius-control); background: var(--color-paper-2); color: var(--color-muted); font-size: var(--text-xs); }
  .view-policy code { color: var(--color-ink); font-family: var(--font-mono); }
  .machine-index { padding-top: var(--space-sm); border-top: var(--rule-strong) solid var(--color-ink); }
  .machine-grid { display: grid; width: 100%; grid-template-columns: minmax(0, var(--machine-card-max)); align-items: start; justify-content: center; gap: var(--space-lg); }
  .machine-slot { width: 100%; min-width: 0; align-self: start; }
  .unreported { display: flex; width: 100%; max-width: var(--machine-card-max); min-height: var(--machine-card-collapsed-height); flex-direction: column; justify-content: space-between; border: var(--rule-hairline) solid var(--color-rule); border-radius: var(--radius-panel); padding: var(--space-md); background: var(--color-paper); }
  .unreported.error { border-color: var(--color-danger-rule); background: var(--color-danger-surface); }
  .unreported div { display: flex; flex-direction: column; gap: var(--space-3xs); }
  .unreported strong { color: var(--color-ink); font-family: var(--font-display); font-size: var(--text-base); }
  .unreported span,
  .unreported p { color: var(--color-muted); font-family: var(--font-mono); font-size: var(--text-xs); }
  .unreported p { margin: 0; line-height: 1.5; }
  .empty-state { display: grid; min-height: 22rem; place-items: center; align-content: center; text-align: center; }
  .empty-state img { display: block; }
  .empty-state h2 { margin: var(--space-md) 0 0; color: var(--color-ink); font-family: var(--font-display); font-size: var(--text-base); }
  .empty-state p { max-width: 48ch; margin: var(--space-xs) var(--space-md) 0; color: var(--color-muted); font-size: var(--text-sm); line-height: 1.55; }
  @media (hover: hover) and (pointer: fine) {
    .brand:hover { color: var(--color-accent); }
  }
  @media (max-width: 47.99rem) {
    .topbar { min-height: 3.5rem; }
    main { padding-top: var(--space-lg); }
    .fleet-head { align-items: flex-end; gap: var(--space-md); }
    .fleet-counts { gap: var(--space-md); }
    .fleet-counts > div { min-width: 3.25rem; }
  }
  @media (min-width: 112rem) {
    .machine-grid { grid-template-columns: repeat(2, minmax(0, 1fr)); }
  }
</style>
