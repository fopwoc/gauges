<script lang="ts">
  import { cubicOut } from 'svelte/easing';
  import { prefersReducedMotion } from 'svelte/motion';
  import CapacityBar from './CapacityBar.svelte';
  import DistroMark from './DistroMark.svelte';
  import MachineHistory from './MachineHistory.svelte';
  import PercentageGauge from './PercentageGauge.svelte';
  import {
    formatBytes,
    formatDuration,
    formatRate,
    formatTemperature,
    relativeTime,
    summarizeStorage,
    usagePercent
  } from '$lib/metrics';
  import type { DeviceHistoryState, DeviceSummary } from '$lib/types';

  const EXPANSION_DURATION_MS = 420;

  let {
    device,
    history,
    serverTimeMs,
    isOpen,
    forceExpanded,
    onToggle
  }: {
    device: DeviceSummary;
    history: DeviceHistoryState;
    serverTimeMs: number;
    isOpen: boolean;
    forceExpanded: boolean;
    onToggle: (deviceId: string, open: boolean) => void;
  } = $props();

  let latest = $derived(device.latestSample);
  let memoryPercent = $derived(latest
    ? usagePercent(latest.memory.usedBytes, latest.memory.totalBytes)
    : null
  );
  let networkReceive = $derived(
    latest?.networks.reduce((sum, network) => sum + network.receivedBytesPerSecond, 0) ?? 0
  );
  let networkTransmit = $derived(
    latest?.networks.reduce((sum, network) => sum + network.transmittedBytesPerSecond, 0) ?? 0
  );
  let storage = $derived(summarizeStorage(latest?.storage ?? []));
  let gpuUsage = $derived.by(() => {
    const values = (latest?.gpus ?? [])
      .map((gpu) => gpu.usagePercent)
      .filter((value): value is number => value != null);
    return values.length > 0 ? Math.max(...values) : null;
  });
  let gpuTemperature = $derived.by(() => {
    const values = (latest?.gpus ?? [])
      .map((gpu) => gpu.temperatureCelsius)
      .filter((value): value is number => value != null);
    return values.length > 0 ? Math.max(...values) : null;
  });
  let panelId = $derived(`machine-panel-${device.deviceId.replace(/[^a-zA-Z0-9_-]/g, '-')}`);
  let stateText = $derived(device.online
    ? `Uptime ${latest ? formatDuration(latest.uptimeSeconds) : '—'}`
    : `Last metric ${relativeTime(serverTimeMs, device.lastMetricTimeMs)}`
  );

  function adaptiveSlide(node: HTMLElement) {
    let initialized = false;
    let previousProgress = 0;
    let renderedHeight = 0;

    return {
      duration: prefersReducedMotion.current ? 0 : EXPANSION_DURATION_MS,
      easing: cubicOut,
      tick(progress: number) {
        if (!initialized) {
          initialized = true;
          previousProgress = progress;
          renderedHeight = node.scrollHeight * progress;
        } else if (progress >= previousProgress) {
          const remainingProgress = 1 - previousProgress;
          const step = remainingProgress <= 0
            ? 1
            : (progress - previousProgress) / remainingProgress;
          renderedHeight += (node.scrollHeight - renderedHeight) * step;
        } else {
          renderedHeight *= previousProgress <= 0 ? 0 : progress / previousProgress;
        }

        node.style.opacity = `${progress}`;
        node.style.height = progress === 1 ? '' : `${Math.max(0, renderedHeight)}px`;
        previousProgress = progress;
      }
    };
  }
</script>

<article class="machine" class:offline={!device.online} class:open={isOpen}>
  <header class="machine-head">
    {#if forceExpanded}
      <div class="identity-control forced">
        <DistroMark distro={device.identity.distro} size={36} />
        <span class="identity-copy">
          <strong>{device.displayName}</strong>
          <span class="identity-meta">
            {#if device.identity.ipAddress}<small>{device.identity.ipAddress}</small>{/if}
            <span class="machine-state" class:online={device.online}>
              <i aria-hidden="true"></i>
              <span>{stateText}</span>
            </span>
          </span>
        </span>
      </div>
    {:else}
      <button
        class="identity-control"
        type="button"
        aria-expanded={isOpen}
        aria-controls={panelId}
        onclick={() => onToggle(device.deviceId, !isOpen)}
      >
        <DistroMark distro={device.identity.distro} size={36} />
        <span class="identity-copy">
          <strong>{device.displayName}</strong>
          <span class="identity-meta">
            {#if device.identity.ipAddress}<small>{device.identity.ipAddress}</small>{/if}
            <span class="machine-state" class:online={device.online}>
              <i aria-hidden="true"></i>
              <span>{stateText}</span>
            </span>
          </span>
        </span>
        <i class="chevron" aria-hidden="true"></i>
      </button>
    {/if}
  </header>

  {#if latest}
    <section class="live" aria-label="Current machine metrics">
      <div class="gauges">
        <PercentageGauge
          label="CPU"
          value={latest.cpu.usagePercent}
          detail={formatTemperature(latest.cpu.temperatureCelsius)}
          meta="processor"
        />
        <PercentageGauge
          label="Memory"
          value={memoryPercent}
          detail={`${formatBytes(latest.memory.usedBytes)} used`}
          meta={formatBytes(latest.memory.totalBytes)}
        />
        {#if gpuUsage != null}
          <PercentageGauge
            label={latest.gpus.length > 1 ? `GPU ×${latest.gpus.length}` : 'GPU'}
            value={gpuUsage}
            detail={formatTemperature(gpuTemperature)}
            meta={latest.gpus.length > 1 ? 'highest utilization' : latest.gpus[0]?.device}
          />
        {/if}
      </div>

      <div class="live-details">
        {#if storage}
          <CapacityBar
            label="Storage"
            value={storage.usagePercent}
            primary={`${formatBytes(storage.usedBytes)} / ${formatBytes(storage.totalBytes)}`}
            secondary={`${storage.poolCount} storage ${storage.poolCount === 1 ? 'pool' : 'pools'} · fullest ${storage.fullestFilesystem?.mountPoint ?? storage.fullest.label} ${(storage.fullestFilesystem?.usagePercent ?? storage.fullest.usagePercent).toFixed(0)}%`}
          />
        {/if}
        <dl class="numbers">
          <div>
            <dt>Network</dt>
            <dd><strong>↓ {formatRate(networkReceive)}</strong><span>↑ {formatRate(networkTransmit)}</span></dd>
          </div>
          {#if latest.memory.swapTotalBytes > 0}
            <div>
              <dt>Swap</dt>
              <dd><strong>{usagePercent(latest.memory.swapUsedBytes, latest.memory.swapTotalBytes).toFixed(0)}%</strong><span>{formatBytes(latest.memory.swapUsedBytes)} used</span></dd>
            </div>
          {/if}
          {#if latest.powerWatts != null}
            <div>
              <dt>Power</dt>
              <dd><strong>{latest.powerWatts.toFixed(1)} W</strong><span>package</span></dd>
            </div>
          {/if}
        </dl>
      </div>
    </section>
  {:else}
    <div class="awaiting" role="status">
      <strong>Awaiting current sample</strong>
      <span>The probe identity is known, but no retained metric is available.</span>
    </div>
  {/if}

  <div id={panelId} aria-hidden={!isOpen} inert={!isOpen}>
    {#if isOpen}
      <div
        class="expanded-motion"
        transition:adaptiveSlide
      >
        <div class="expanded-clip">
          {#if history.initialized}
            <MachineHistory
              {device}
              {history}
              {serverTimeMs}
            />
          {:else if history.error}
            <div class="history-error" role="status">
              <strong>History unavailable</strong>
              <span>{history.error}</span>
            </div>
          {:else}
            <div class="history-loading" aria-label="Loading machine history" aria-busy="true">
              <i></i><i></i>
            </div>
          {/if}
        </div>
      </div>
    {/if}
  </div>
</article>

<style>
  /* Hallmark · pre-emit critique: P5 H5 E5 S5 R5 V4
   * component: live machine workbench · genre: modern-minimal · theme: Cobalt
   * interaction: independent accordion · loading: exact-dimension shimmer
   */
  .machine {
    width: 100%;
    max-width: var(--machine-card-max);
    align-self: start;
    overflow: clip;
    border: var(--rule-hairline) solid var(--color-rule);
    border-radius: var(--radius-panel);
    background: var(--color-paper);
    box-shadow: var(--shadow-raised);
    transition: border-color var(--dur-short) var(--ease-out), opacity var(--dur-short) var(--ease-out);
  }
  .machine.offline { border-color: var(--color-danger-rule); }
  .machine-head { min-width: 0; padding: var(--space-sm) var(--space-md); border-bottom: var(--rule-hairline) solid var(--color-rule); }
  .identity-control { display: grid; width: 100%; min-width: 0; grid-template-columns: auto minmax(0, 1fr) auto; align-items: center; gap: var(--space-sm); border: 0; border-radius: var(--radius-control); padding: var(--space-2xs); margin: calc(var(--space-2xs) * -1); background: transparent; text-align: left; cursor: pointer; }
  .identity-control.forced { cursor: default; }
  .identity-control:active:not(.forced) { background: var(--color-paper-3); }
  .identity-copy { display: flex; min-width: 0; flex-direction: column; gap: var(--space-3xs); }
  .identity-meta { display: flex; min-width: 0; flex-wrap: wrap; align-items: center; gap: var(--space-2xs) var(--space-sm); }
  .identity-copy strong { overflow: hidden; color: var(--color-ink); font-family: var(--font-display); font-size: var(--text-base); font-weight: 680; letter-spacing: -0.025em; text-overflow: ellipsis; white-space: nowrap; }
  .identity-copy small { overflow: hidden; color: var(--color-muted); font-family: var(--font-mono); font-size: var(--text-xs); text-overflow: ellipsis; white-space: nowrap; }
  .chevron { width: 0.55rem; height: 0.55rem; margin-inline-end: var(--space-2xs); border-right: var(--rule-strong) solid var(--color-muted); border-bottom: var(--rule-strong) solid var(--color-muted); transform: rotate(45deg); transition: transform var(--dur-short) var(--ease-out); }
  .machine.open .chevron { transform: rotate(225deg); }
  .machine-state { display: inline-flex; flex: 0 0 auto; align-items: center; gap: var(--space-xs); color: var(--color-danger); font-family: var(--font-mono); font-size: var(--text-xs); white-space: nowrap; }
  .machine-state.online { color: var(--color-accent); }
  .machine-state > i { width: 0.45rem; height: 0.45rem; border-radius: 50%; background: currentColor; box-shadow: 0 0 0 var(--space-3xs) color-mix(in oklch, currentColor 16%, transparent); }
  .live { display: grid; min-height: calc(var(--machine-card-collapsed-height) - 3.75rem); grid-template-columns: minmax(17rem, 1.15fr) minmax(16rem, 1fr); gap: var(--space-lg); padding: var(--space-md); }
  .gauges { display: grid; min-width: 0; grid-template-columns: repeat(auto-fit, minmax(5.5rem, 1fr)); align-items: start; gap: var(--space-sm); }
  .live-details { display: flex; min-width: 0; flex-direction: column; justify-content: center; gap: var(--space-md); border-inline-start: var(--rule-hairline) solid var(--color-rule); padding-inline-start: var(--space-lg); }
  .numbers { display: grid; grid-template-columns: repeat(auto-fit, minmax(6.5rem, 1fr)); gap: var(--space-md); margin: 0; }
  .numbers div { min-width: 0; }
  .numbers dt { color: var(--color-muted); font-family: var(--font-display); font-size: var(--text-xs); font-weight: 620; }
  .numbers dd { display: flex; min-width: 0; flex-direction: column; gap: var(--space-3xs); margin: var(--space-2xs) 0 0; font-family: var(--font-mono); font-size: var(--text-xs); font-variant-numeric: tabular-nums; }
  .numbers strong { overflow: hidden; color: var(--color-ink); text-overflow: ellipsis; white-space: nowrap; }
  .numbers span { overflow: hidden; color: var(--color-muted); text-overflow: ellipsis; white-space: nowrap; }
  .awaiting { display: flex; min-height: calc(var(--machine-card-collapsed-height) - 3.75rem); flex-direction: column; align-items: center; justify-content: center; gap: var(--space-xs); padding: var(--space-lg); color: var(--color-muted); text-align: center; }
  .awaiting strong { color: var(--color-ink); font-family: var(--font-display); font-size: var(--text-sm); }
  .awaiting span { max-width: 42ch; font-size: var(--text-xs); line-height: 1.5; }
  .expanded-motion { overflow: hidden; }
  .expanded-clip { min-height: 0; overflow: hidden; }
  .history-loading { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: var(--space-sm); padding: var(--space-lg) var(--space-md) var(--space-md); border-top: var(--rule-hairline) solid var(--color-rule); }
  .history-loading i { height: 16.5rem; border-radius: var(--radius-control); background: linear-gradient(100deg, var(--color-paper-2), var(--color-paper-3), var(--color-paper-2)); background-size: 220% 100%; animation: shimmer 1.4s linear infinite; }
  .history-error { display: flex; flex-direction: column; gap: var(--space-xs); padding: var(--space-lg); border-top: var(--rule-hairline) solid var(--color-danger-rule); background: var(--color-danger-surface); color: var(--color-danger-ink); }
  .history-error strong { font-family: var(--font-display); font-size: var(--text-sm); }
  .history-error span { font-size: var(--text-xs); }
  @keyframes shimmer { to { background-position: -120% 0; } }
  @media (hover: hover) and (pointer: fine) { .identity-control:hover:not(.forced) { background: var(--color-paper-2); } }
  @media (max-width: 40rem) {
    .identity-control { grid-template-columns: auto minmax(0, 1fr) auto; row-gap: var(--space-2xs); }
    .live { grid-template-columns: 1fr; }
    .live-details { border-inline-start: 0; border-top: var(--rule-hairline) solid var(--color-rule); padding: var(--space-md) 0 0; }
    .history-loading { grid-template-columns: 1fr; }
  }
  @media (prefers-reduced-motion: reduce) {
    .chevron { transition: none; }
    .history-loading i { animation: none; }
  }
</style>
