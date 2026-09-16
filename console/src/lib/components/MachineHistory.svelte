<script lang="ts">
  import CapacityBar from './CapacityBar.svelte';
  import HistoryChart from './HistoryChart.svelte';
  import {
    formatBytes,
    formatBitsPerSecond,
    formatPercent,
    formatTemperature,
    toBitsPerSecond,
    usagePercent
  } from '$lib/metrics';
  import type { ChartSeries, DeviceHistoryState, DeviceSummary, StorageMetric } from '$lib/types';

  const HOUR_MS = 3_600_000;
  const DAY_MS = 24 * HOUR_MS;

  let {
    device,
    history,
    serverTimeMs
  }: {
    device: DeviceSummary;
    history: DeviceHistoryState;
    serverTimeMs: number;
  } = $props();

  let latest = $derived(device.latestSample ?? history.samples.at(-1) ?? null);
  let sampleTimesMs = $derived(history.samples.map((sample) => sample.capturedAtMs));
  let chartNowMs = $derived(serverTimeMs);
  let expectedIntervalMs = $derived(
    (history.collectionIntervalSeconds ?? device.collectionIntervalSeconds) * 1_000
  );
  let selectedTimeMs = $state<number | null>(null);
  let windowDurationMs = $state(HOUR_MS);
  let viewEndMs = $state(0);
  let followingLatest = $state(true);

  function selectTime(timeMs: number | null): void {
    selectedTimeMs = timeMs;
  }

  function setViewport(endMs: number, shouldFollowLatest: boolean): void {
    viewEndMs = endMs;
    followingLatest = shouldFollowLatest;
  }

  function toggleWindow(): void {
    windowDurationMs = windowDurationMs === HOUR_MS ? DAY_MS : HOUR_MS;
  }

  function storageKind(storage: StorageMetric): string {
    if (storage.kind === 'btrfs') return 'Btrfs';
    if (storage.kind === 'ubi') return 'UBIFS';
    if (storage.kind === 'filesystem') return storage.fileSystem;
    return storage.fileSystems.join(' / ') || 'disk';
  }

  function storageDetails(storage: StorageMetric): string {
    const fullest = storage.fullestFilesystem;
    const constraint = fullest != null
      && (storage.mountPoints.length > 1 || fullest.usagePercent > storage.usagePercent + 0.5)
        ? `fullest ${fullest.mountPoint} ${fullest.usagePercent.toFixed(0)}%`
        : null;
    if (storage.kind === 'disk') {
      const mounts = storage.mountPoints.length === 1
        ? storage.mountPoints[0]
        : `${storage.mountPoints.length} filesystems`;
      return [storage.device, storage.fileSystems.join(' / '), mounts, constraint]
        .filter((value): value is string => Boolean(value)).join(' · ');
    }
    if (storage.kind === 'filesystem') {
      return `${storage.source} · ${storage.fileSystem}`;
    }
    if (storage.kind === 'btrfs') {
      const errors = storage.deviceErrors
        ? Object.values(storage.deviceErrors).reduce((sum, value) => sum + value, 0)
        : null;
      const profile = storage.dataProfiles.join(' + ') || 'unknown profile';
      return [
        `Btrfs ${profile}`,
        `${storage.devices.length} ${storage.devices.length === 1 ? 'device' : 'devices'}`,
        storage.logicalBytes === storage.totalBytes ? null : `${formatBytes(storage.logicalBytes)} usable`,
        storage.physicalBytes === storage.logicalBytes ? null : `${formatBytes(storage.physicalBytes)} raw`,
        errors == null ? null : `${errors.toLocaleString()} Btrfs device errors`,
        constraint
      ].filter((value): value is string => value != null).join(' · ');
    }
    const health = storage.mtdHealth;
    return [
      `UBIFS ${storage.ubiDevice}${storage.mtdDevice ? ` / ${storage.mtdDevice}` : ''}`,
      storage.maxEraseCount == null ? null : `max EC ${storage.maxEraseCount.toLocaleString()}`,
      storage.badPebs == null ? null : `${storage.badPebs.toLocaleString()} bad PEB`,
      health?.correctedBits == null ? null : `${health.correctedBits.toLocaleString()} corrected bits`,
      health?.eccFailures == null ? null : `${health.eccFailures.toLocaleString()} ECC failures`,
      storage.corrupted ? 'CORRUPTED' : null,
      storage.readOnly ? 'read-only' : null,
      constraint
    ].filter((value): value is string => value != null).join(' · ');
  }

  $effect(() => {
    if (followingLatest) viewEndMs = serverTimeMs;
  });

  let cpuUsageSeries = $derived<ChartSeries[]>([{
    label: 'Usage',
    points: history.samples.map((sample) => ({ timeMs: sample.capturedAtMs, value: sample.cpu.usagePercent })),
    format: formatPercent
  }]);
  let cpuTemperatureSeries = $derived<ChartSeries[]>([{
    label: 'Temperature',
    points: history.samples.map((sample) => ({ timeMs: sample.capturedAtMs, value: sample.cpu.temperatureCelsius })),
    format: formatTemperature
  }]);
  let memorySeries = $derived<ChartSeries[]>([
    {
      label: 'RAM',
      points: history.samples.map((sample) => ({
        timeMs: sample.capturedAtMs,
        value: usagePercent(sample.memory.usedBytes, sample.memory.totalBytes)
      })),
      format: formatPercent
    },
    {
      label: 'Swap',
      points: history.samples.map((sample) => ({
        timeMs: sample.capturedAtMs,
        value: sample.memory.swapTotalBytes > 0
          ? usagePercent(sample.memory.swapUsedBytes, sample.memory.swapTotalBytes)
          : null
      })),
      format: formatPercent
    }
  ]);
  let powerSeries = $derived<ChartSeries[]>([{
    label: 'Power',
    points: history.samples.map((sample) => ({ timeMs: sample.capturedAtMs, value: sample.powerWatts })),
    format: (value) => `${value.toFixed(1)} W`
  }]);

  let storageCharts = $derived.by(() => {
    const storage = new Map<string, { label: string; kind: string }>();
    for (const sample of history.samples) {
      for (const pool of sample.storage) {
        storage.set(pool.id, { label: pool.label, kind: storageKind(pool) });
      }
    }
    for (const pool of latest?.storage ?? []) {
      storage.set(pool.id, { label: pool.label, kind: storageKind(pool) });
    }
    return [...storage.entries()]
      .sort((left, right) => left[0].localeCompare(right[0]))
      .map(([key, pool]) => ({
        key,
        title: pool.label,
        subtitle: `${pool.kind} · capacity used`,
        series: [{
          label: 'Used',
          points: history.samples.map((sample) => ({
            timeMs: sample.capturedAtMs,
            value: sample.storage.find((item) => item.id === key)?.usagePercent ?? null
          })),
          format: formatPercent
        }] satisfies ChartSeries[]
      }));
  });

  let driveTemperatureSeries = $derived.by(() => {
    const devices = new Set(history.samples.flatMap((sample) =>
      (sample.driveTemperatures ?? []).map((drive) => drive.device)
    ));
    for (const drive of latest?.driveTemperatures ?? []) devices.add(drive.device);
    return [...devices].sort().map((device) => ({
      label: device,
      points: history.samples.map((sample) => ({
        timeMs: sample.capturedAtMs,
        value: sample.driveTemperatures?.find((drive) => drive.device === device)?.temperatureCelsius ?? null
      })),
      format: formatTemperature
    } satisfies ChartSeries));
  });

  let networkCharts = $derived.by(() => {
    const names = new Set(history.samples.flatMap((sample) => sample.networks.map((network) => network.interface)));
    if (latest) for (const network of latest.networks) names.add(network.interface);
    return [...names].sort().map((name) => ({
      name,
      series: [
        {
          label: 'Receive',
          points: history.samples.map((sample) => ({
            timeMs: sample.capturedAtMs,
            value: toBitsPerSecond(
              sample.networks.find((network) => network.interface === name)?.receivedBytesPerSecond
            )
          })),
          format: formatBitsPerSecond
        },
        {
          label: 'Transmit',
          points: history.samples.map((sample) => ({
            timeMs: sample.capturedAtMs,
            value: toBitsPerSecond(
              sample.networks.find((network) => network.interface === name)?.transmittedBytesPerSecond
            )
          })),
          format: formatBitsPerSecond
        }
      ] satisfies ChartSeries[]
    }));
  });

  let gpuCharts = $derived.by(() => {
    const devices = new Set(history.samples.flatMap((sample) => sample.gpus.map((gpu) => gpu.device)));
    if (latest) for (const gpu of latest.gpus) devices.add(gpu.device);
    return [...devices].sort().map((gpuDevice) => {
      const usagePoints = history.samples.map((sample) => ({
        timeMs: sample.capturedAtMs,
        value: sample.gpus.find((gpu) => gpu.device === gpuDevice)?.usagePercent ?? null
      }));
      const vramPoints = history.samples.map((sample) => {
        const gpu = sample.gpus.find((item) => item.device === gpuDevice);
        return {
          timeMs: sample.capturedAtMs,
          value: gpu?.vramUsedBytes != null && gpu.vramTotalBytes
            ? usagePercent(gpu.vramUsedBytes, gpu.vramTotalBytes)
            : null
        };
      });
      const temperaturePoints = history.samples.map((sample) => ({
        timeMs: sample.capturedAtMs,
        value: sample.gpus.find((gpu) => gpu.device === gpuDevice)?.temperatureCelsius ?? null
      }));
      const percentageSeries: ChartSeries[] = [];
      if (usagePoints.some((point) => point.value != null)) {
        percentageSeries.push({ label: 'Usage', points: usagePoints, format: formatPercent });
      }
      if (vramPoints.some((point) => point.value != null)) {
        percentageSeries.push({ label: 'VRAM', points: vramPoints, format: formatPercent });
      }
      return {
        device: gpuDevice,
        percentageSeries,
        temperatureSeries: temperaturePoints.some((point) => point.value != null)
          ? [{ label: 'Temperature', points: temperaturePoints, format: formatTemperature }] satisfies ChartSeries[]
          : []
      };
    });
  });
</script>

<div class="history">
  <header class="system-line">
    <div>
      <span>{device.identity.distro} {device.identity.distroVersion}</span>
      <span>Kernel {device.identity.kernelVersion}</span>
      {#if device.identity.gpuTypes.length > 0}<span>{device.identity.gpuTypes.join(' / ')}</span>{/if}
    </div>
    <span class:error={Boolean(history.error)}>
      {history.loading ? 'Syncing…' : history.error ?? `${history.retentionHours?.toLocaleString() ?? '—'}h retained · ${device.collectionIntervalSeconds.toLocaleString()}s cadence`}
    </span>
  </header>

  {#if latest && (latest.storage.length > 0 || latest.gpus.length > 0)}
    <section class="hardware" aria-labelledby={`hardware-${device.deviceId}`}>
      <div class="section-title">
        <h3 id={`hardware-${device.deviceId}`}>{latest.gpus.length > 0 ? 'Storage and graphics' : 'Storage'}</h3>
      </div>
      {#if latest.storage.length > 0}
        <div class="volume-list">
          {#each latest.storage as storage (storage.id)}
            <div>
              <CapacityBar
                label={storage.label}
                value={storage.usagePercent}
                primary={`${formatBytes(storage.usedBytes)} / ${formatBytes(storage.totalBytes)}`}
                secondary={storageDetails(storage)}
              />
              {#if storage.kind === 'btrfs' && storage.devices.length > 0}
                <div class="smart-health" aria-label={`SMART health for ${storage.label}`}>
                  {#if (storage.smartHealth ?? []).length === 0}
                    <span>SMART · awaiting probe</span>
                  {:else}
                    {#each storage.smartHealth ?? [] as health (health.device)}
                      <span class:failed={health.status === 'failed'} title={health.checkedAtMs == null ? 'Awaiting first check' : `Checked ${new Date(health.checkedAtMs).toLocaleString()}`}>
                        SMART {health.device} · {health.status}
                      </span>
                    {/each}
                  {/if}
                </div>
              {/if}
            </div>
          {/each}
        </div>
        {#if latest.storage.some((storage) => storage.kind === 'btrfs')}
          <p class="storage-note">Btrfs errors are persistent filesystem counters for failed I/O and detected corruption. SMART is a separate drive-reported check; passed does not guarantee a healthy drive. Pending awaits the first check, standby was skipped, and unavailable could not be read.</p>
        {/if}
      {/if}
      {#if latest.gpus.length > 0}
        <dl class="gpu-list">
          {#each latest.gpus as gpu (gpu.device)}
            <div>
              <dt>{gpu.device}</dt>
              <dd>
                {#if gpu.usagePercent != null}<span>{formatPercent(gpu.usagePercent)} load</span>{/if}
                {#if gpu.temperatureCelsius != null}<span>{formatTemperature(gpu.temperatureCelsius)}</span>{/if}
                {#if gpu.vramUsedBytes != null && gpu.vramTotalBytes}<span>{formatBytes(gpu.vramUsedBytes)} / {formatBytes(gpu.vramTotalBytes)} VRAM</span>{/if}
              </dd>
            </div>
          {/each}
        </dl>
      {/if}
    </section>
  {/if}

  <section class="charts" aria-label="Metric history">
    <div class="chart-list">
      <HistoryChart title="Processor usage" subtitle="percentage" series={cpuUsageSeries} {sampleTimesMs} serverTimeMs={chartNowMs} online={device.online} {expectedIntervalMs} {selectedTimeMs} onSelectedTimeChange={selectTime} {windowDurationMs} {viewEndMs} {followingLatest} onViewportChange={setViewport} onToggleWindow={toggleWindow} scale="percentage" />
      {#if history.samples.some((sample) => sample.cpu.temperatureCelsius != null)}
        <HistoryChart title="Processor temperature" subtitle="degrees Celsius" series={cpuTemperatureSeries} {sampleTimesMs} serverTimeMs={chartNowMs} online={device.online} {expectedIntervalMs} {selectedTimeMs} onSelectedTimeChange={selectTime} {windowDurationMs} {viewEndMs} {followingLatest} onViewportChange={setViewport} onToggleWindow={toggleWindow} />
      {/if}
      <HistoryChart title="Memory" subtitle="RAM / swap used" series={memorySeries} {sampleTimesMs} serverTimeMs={chartNowMs} online={device.online} {expectedIntervalMs} {selectedTimeMs} onSelectedTimeChange={selectTime} {windowDurationMs} {viewEndMs} {followingLatest} onViewportChange={setViewport} onToggleWindow={toggleWindow} scale="percentage" />
      {#if history.samples.some((sample) => sample.powerWatts != null)}
        <HistoryChart title="Package power" subtitle="watts" series={powerSeries} {sampleTimesMs} serverTimeMs={chartNowMs} online={device.online} {expectedIntervalMs} {selectedTimeMs} onSelectedTimeChange={selectTime} {windowDurationMs} {viewEndMs} {followingLatest} onViewportChange={setViewport} onToggleWindow={toggleWindow} />
      {/if}
      {#each storageCharts as storage (storage.key)}
        <HistoryChart title={storage.title} subtitle={storage.subtitle} series={storage.series} {sampleTimesMs} serverTimeMs={chartNowMs} online={device.online} {expectedIntervalMs} {selectedTimeMs} onSelectedTimeChange={selectTime} {windowDurationMs} {viewEndMs} {followingLatest} onViewportChange={setViewport} onToggleWindow={toggleWindow} scale="percentage" />
      {/each}
      {#if driveTemperatureSeries.length > 0}
        <HistoryChart title="Drive temperatures" subtitle="degrees Celsius · one line per drive" series={driveTemperatureSeries} {sampleTimesMs} serverTimeMs={chartNowMs} online={device.online} {expectedIntervalMs} {selectedTimeMs} onSelectedTimeChange={selectTime} {windowDurationMs} {viewEndMs} {followingLatest} onViewportChange={setViewport} onToggleWindow={toggleWindow} />
      {/if}
      {#each networkCharts as network (network.name)}
        <HistoryChart title={network.name} subtitle="network throughput · bit/s" series={network.series} {sampleTimesMs} serverTimeMs={chartNowMs} online={device.online} {expectedIntervalMs} {selectedTimeMs} onSelectedTimeChange={selectTime} {windowDurationMs} {viewEndMs} {followingLatest} onViewportChange={setViewport} onToggleWindow={toggleWindow} />
      {/each}
      {#each gpuCharts as gpu (gpu.device)}
        {#if gpu.percentageSeries.length > 0}
          <HistoryChart title={`${gpu.device} utilization`} subtitle="GPU / VRAM" series={gpu.percentageSeries} {sampleTimesMs} serverTimeMs={chartNowMs} online={device.online} {expectedIntervalMs} {selectedTimeMs} onSelectedTimeChange={selectTime} {windowDurationMs} {viewEndMs} {followingLatest} onViewportChange={setViewport} onToggleWindow={toggleWindow} scale="percentage" />
        {/if}
        {#if gpu.temperatureSeries.length > 0}
          <HistoryChart title={`${gpu.device} temperature`} subtitle="degrees Celsius" series={gpu.temperatureSeries} {sampleTimesMs} serverTimeMs={chartNowMs} online={device.online} {expectedIntervalMs} {selectedTimeMs} onSelectedTimeChange={selectTime} {windowDurationMs} {viewEndMs} {followingLatest} onViewportChange={setViewport} onToggleWindow={toggleWindow} />
        {/if}
      {/each}
    </div>
  </section>
</div>

<style>
  /* Hallmark · component: machine history workbench · genre: modern-minimal · theme: Cobalt */
  .history { border-top: var(--rule-hairline) solid var(--color-rule); padding: var(--space-md); }
  .system-line { display: flex; align-items: flex-start; justify-content: space-between; gap: var(--space-md); padding-bottom: var(--space-md); color: var(--color-muted); font-family: var(--font-mono); font-size: var(--text-xs); }
  .system-line > div { display: flex; min-width: 0; flex-wrap: wrap; gap: var(--space-2xs) var(--space-md); }
  .system-line > span { flex: 0 0 auto; white-space: nowrap; }
  .system-line > span.error { color: var(--color-danger); }
  .hardware { padding-top: var(--space-lg); border-top: var(--rule-hairline) solid var(--color-rule); }
  .hardware + .charts { margin-top: var(--space-lg); }
  .section-title { display: flex; align-items: baseline; justify-content: space-between; gap: var(--space-md); margin-bottom: var(--space-md); }
  .section-title h3 { margin: 0; color: var(--color-ink); font-family: var(--font-display); font-size: var(--text-sm); font-weight: 680; }
  .volume-list { display: grid; grid-template-columns: repeat(auto-fit, minmax(min(100%, 14rem), 1fr)); gap: var(--space-md); }
  .smart-health { display: flex; flex-wrap: wrap; gap: var(--space-2xs) var(--space-sm); margin-top: var(--space-xs); color: var(--color-muted); font-family: var(--font-mono); font-size: var(--text-xs); }
  .smart-health .failed { color: var(--color-danger); font-weight: 650; }
  .storage-note { margin: var(--space-sm) 0 0; color: var(--color-muted); font-family: var(--font-mono); font-size: var(--text-xs); }
  .gpu-list { display: grid; grid-template-columns: repeat(auto-fit, minmax(min(100%, 14rem), 1fr)); gap: var(--space-md); margin: var(--space-lg) 0 0; }
  .gpu-list div { min-width: 0; }
  .gpu-list dt { overflow: hidden; color: var(--color-ink); font-family: var(--font-display); font-size: var(--text-xs); font-weight: 650; text-overflow: ellipsis; white-space: nowrap; }
  .gpu-list dd { display: flex; flex-wrap: wrap; gap: var(--space-2xs) var(--space-sm); margin: var(--space-2xs) 0 0; color: var(--color-muted); font-family: var(--font-mono); font-size: var(--text-xs); }
  .chart-list { display: grid; grid-template-columns: 1fr; gap: var(--space-sm); }
  @media (max-width: 40rem) {
    .system-line,
    .section-title { align-items: flex-start; flex-direction: column; }
  }
</style>
