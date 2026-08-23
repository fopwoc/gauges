<script lang="ts">
  import CapacityBar from './CapacityBar.svelte';
  import HistoryChart from './HistoryChart.svelte';
  import {
    formatBytes,
    formatPercent,
    formatRate,
    formatTemperature,
    usagePercent
  } from '$lib/metrics';
  import type { ChartSeries, DeviceHistoryState, DeviceSummary } from '$lib/types';

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

  let diskCharts = $derived.by(() => {
    const disks = new Map<string, { device: string; mountPoint: string }>();
    for (const sample of history.samples) {
      for (const disk of sample.disks) {
        disks.set(`${disk.device}:${disk.mountPoint}`, { device: disk.device, mountPoint: disk.mountPoint });
      }
    }
    for (const disk of latest?.disks ?? []) {
      disks.set(`${disk.device}:${disk.mountPoint}`, { device: disk.device, mountPoint: disk.mountPoint });
    }
    return [...disks.entries()]
      .sort((left, right) => left[0].localeCompare(right[0]))
      .map(([key, disk]) => ({
        key,
        title: disk.mountPoint,
        subtitle: `${disk.device} · capacity used`,
        series: [{
          label: 'Used',
          points: history.samples.map((sample) => ({
            timeMs: sample.capturedAtMs,
            value: sample.disks.find((item) => `${item.device}:${item.mountPoint}` === key)?.usagePercent ?? null
          })),
          format: formatPercent
        }] satisfies ChartSeries[]
      }));
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
            value: sample.networks.find((network) => network.interface === name)?.receivedBytesPerSecond ?? null
          })),
          format: formatRate
        },
        {
          label: 'Transmit',
          points: history.samples.map((sample) => ({
            timeMs: sample.capturedAtMs,
            value: sample.networks.find((network) => network.interface === name)?.transmittedBytesPerSecond ?? null
          })),
          format: formatRate
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

  {#if latest && (latest.disks.length > 0 || latest.gpus.length > 0)}
    <section class="hardware" aria-labelledby={`hardware-${device.deviceId}`}>
      <div class="section-title">
        <h3 id={`hardware-${device.deviceId}`}>Logical hardware</h3>
        <span>filesystem-visible capacity · optional accelerators</span>
      </div>
      {#if latest.disks.length > 0}
        <div class="volume-list">
          {#each latest.disks as disk (`${disk.device}:${disk.mountPoint}`)}
            <CapacityBar
              label={disk.mountPoint}
              value={disk.usagePercent}
              primary={`${formatBytes(disk.usedBytes)} / ${formatBytes(disk.totalBytes)}`}
              secondary={`${disk.device} · ${disk.fileSystem}`}
            />
          {/each}
        </div>
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
      {#each diskCharts as disk (disk.key)}
        <HistoryChart title={disk.title} subtitle={disk.subtitle} series={disk.series} {sampleTimesMs} serverTimeMs={chartNowMs} online={device.online} {expectedIntervalMs} {selectedTimeMs} onSelectedTimeChange={selectTime} {windowDurationMs} {viewEndMs} {followingLatest} onViewportChange={setViewport} onToggleWindow={toggleWindow} scale="percentage" />
      {/each}
      {#each networkCharts as network (network.name)}
        <HistoryChart title={network.name} subtitle="network throughput" series={network.series} {sampleTimesMs} serverTimeMs={chartNowMs} online={device.online} {expectedIntervalMs} {selectedTimeMs} onSelectedTimeChange={selectTime} {windowDurationMs} {viewEndMs} {followingLatest} onViewportChange={setViewport} onToggleWindow={toggleWindow} />
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
  .section-title span { color: var(--color-muted); font-family: var(--font-mono); font-size: var(--text-xs); text-align: right; }
  .volume-list { display: grid; grid-template-columns: repeat(auto-fit, minmax(min(100%, 14rem), 1fr)); gap: var(--space-md); }
  .gpu-list { display: grid; grid-template-columns: repeat(auto-fit, minmax(min(100%, 14rem), 1fr)); gap: var(--space-md); margin: var(--space-lg) 0 0; }
  .gpu-list div { min-width: 0; }
  .gpu-list dt { overflow: hidden; color: var(--color-ink); font-family: var(--font-display); font-size: var(--text-xs); font-weight: 650; text-overflow: ellipsis; white-space: nowrap; }
  .gpu-list dd { display: flex; flex-wrap: wrap; gap: var(--space-2xs) var(--space-sm); margin: var(--space-2xs) 0 0; color: var(--color-muted); font-family: var(--font-mono); font-size: var(--text-xs); }
  .chart-list { display: grid; grid-template-columns: 1fr; gap: var(--space-sm); }
  @media (max-width: 40rem) {
    .system-line,
    .section-title { align-items: flex-start; flex-direction: column; }
    .section-title span { text-align: left; }
  }
</style>
