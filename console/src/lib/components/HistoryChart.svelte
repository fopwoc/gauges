<script lang="ts">
  import { onMount } from 'svelte';
  import type uPlot from 'uplot';
  import {
    clampChartWindowEnd,
    findMissingIntervals,
    valueAtLatestSample,
    wheelDeltaToTimeMs
  } from '$lib/metrics';
  import type { ChartScale, ChartSeries, MissingInterval } from '$lib/types';

  const axisTimeFormatter = new Intl.DateTimeFormat(undefined, {
    hour: '2-digit',
    minute: '2-digit',
    hourCycle: 'h23'
  });
  const readoutTimeFormatter = new Intl.DateTimeFormat(undefined, {
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
    hourCycle: 'h23'
  });

  let {
    title,
    subtitle,
    series,
    sampleTimesMs,
    serverTimeMs,
    online,
    expectedIntervalMs,
    selectedTimeMs,
    onSelectedTimeChange,
    windowDurationMs,
    viewEndMs,
    followingLatest,
    onViewportChange,
    onToggleWindow,
    scale = 'automatic',
    height = 176,
    emptyLabel = 'Waiting for samples'
  }: {
    title: string;
    subtitle?: string;
    series: ChartSeries[];
    sampleTimesMs: number[];
    serverTimeMs: number;
    online: boolean;
    expectedIntervalMs: number;
    selectedTimeMs: number | null;
    onSelectedTimeChange: (timeMs: number | null) => void;
    windowDurationMs: number;
    viewEndMs: number;
    followingLatest: boolean;
    onViewportChange: (endMs: number, followingLatest: boolean) => void;
    onToggleWindow: () => void;
    scale?: ChartScale;
    height?: number;
    emptyLabel?: string;
  } = $props();

  let host: HTMLDivElement;
  let chart: uPlot | undefined;
  let UPlot: typeof uPlot | undefined;
  let hovered = $state<{ timeMs: number; values: Array<number | null> } | null>(null);
  let schemeQuery: MediaQueryList | undefined;
  let reducedMotionQuery: MediaQueryList | undefined;
  let renderedScale: ChartScale | undefined;
  let renderedWindowDurationMs: number | undefined;
  let scaleAnimationFrame: number | undefined;

  let missingIntervals = $derived(
    findMissingIntervals(sampleTimesMs, serverTimeMs, expectedIntervalMs, online)
  );
  let hasData = $derived(series.some((item) => item.points.some((point) => point.value != null)));
  let earliestSampleMs = $derived.by(() => {
    let earliest = Number.POSITIVE_INFINITY;
    for (const timestamp of sampleTimesMs) earliest = Math.min(earliest, timestamp);
    return earliest;
  });

  function cssToken(name: string): string {
    const styles = getComputedStyle(host);
    return styles.getPropertyValue(name).trim() || styles.color;
  }

  function currentValue(item: ChartSeries): number | null {
    return valueAtLatestSample(item.points, sampleTimesMs);
  }

  function shownValue(item: ChartSeries, index: number): string {
    const value = hovered ? hovered.values[index] : currentValue(item);
    return value == null ? '—' : (item.format?.(value) ?? value.toFixed(1));
  }

  function shownTime(): string {
    if (!hovered) return '';
    return readoutTimeFormatter.format(hovered.timeMs);
  }

  function visibleEnd(): number {
    return clampChartWindowEnd(
      viewEndMs,
      earliestSampleMs,
      serverTimeMs,
      windowDurationMs
    );
  }

  function visibleScale(): { min: number; max: number } {
    const endMs = visibleEnd();
    return { min: (endMs - windowDurationMs) / 1_000, max: endMs / 1_000 };
  }

  function alignedData(items: ChartSeries[], gaps: MissingInterval[]): uPlot.AlignedData {
    const timestamps = new Set(items.flatMap((item) => item.points.map((point) => point.timeMs)));
    for (const gap of gaps) {
      timestamps.add(gap.startMs);
      timestamps.add(gap.endMs);
    }
    const scale = visibleScale();
    timestamps.add(scale.min * 1_000);
    timestamps.add(scale.max * 1_000);

    const sorted = [...timestamps].sort((a, b) => a - b);
    const values = items.map((item) => {
      const lookup = new Map(item.points.map((point) => [point.timeMs, point.value]));
      return sorted.map((timestamp) => lookup.get(timestamp) ?? null);
    });
    return [sorted.map((timestamp) => timestamp / 1_000), ...values] as uPlot.AlignedData;
  }

  function shadeGaps(plot: uPlot, gaps: MissingInterval[]): void {
    const fill = cssToken('--color-danger-surface');
    const edge = cssToken('--color-danger');
    const { ctx, bbox } = plot;
    ctx.save();
    for (const gap of gaps) {
      const start = Math.max(bbox.left, plot.valToPos(gap.startMs / 1_000, 'x', true));
      const end = Math.min(bbox.left + bbox.width, plot.valToPos(gap.endMs / 1_000, 'x', true));
      if (end <= start) continue;
      ctx.fillStyle = fill;
      ctx.fillRect(start, bbox.top, end - start, bbox.height);
      ctx.strokeStyle = edge;
      ctx.globalAlpha = gap.kind === 'stale' ? 0.48 : 0.28;
      ctx.lineWidth = 1;
      ctx.beginPath();
      ctx.moveTo(start, bbox.top);
      ctx.lineTo(start, bbox.top + bbox.height);
      ctx.stroke();
      ctx.globalAlpha = 1;
    }
    ctx.restore();
  }

  function synchronizeSelection(timeMs: number | null): void {
    if (!chart) return;
    const minimum = chart.scales.x.min;
    const maximum = chart.scales.x.max;
    if (
      timeMs == null
      || minimum == null
      || maximum == null
      || timeMs / 1_000 < minimum
      || timeMs / 1_000 > maximum
    ) {
      hovered = null;
      chart.setCursor({ left: -10, top: -10 }, false);
      return;
    }

    const index = chart.valToIdx(timeMs / 1_000);
    hovered = {
      timeMs,
      values: series.map((_item, seriesIndex) => chart?.data[seriesIndex + 1][index] ?? null)
    };
    chart.setCursor({
      left: chart.valToPos(timeMs / 1_000, 'x'),
      top: chart.bbox.height / 2
    }, false);
  }

  function buildChart(): void {
    if (!UPlot || !host || host.clientWidth === 0) return;
    hovered = null;
    chart?.destroy();
    chart = undefined;
    if (!hasData) return;

    const palette = [
      cssToken('--color-chart-1'),
      cssToken('--color-chart-2'),
      cssToken('--color-chart-3')
    ];
    const fills = [
      cssToken('--color-chart-1-fill'),
      cssToken('--color-chart-2-fill'),
      cssToken('--color-chart-3-fill')
    ];
    const axis = cssToken('--color-muted');
    const grid = cssToken('--color-chart-grid');
    const chartFont = cssToken('--font-mono');
    const data = alignedData(series, missingIntervals);
    const valueScaleKey = scale === 'percentage' ? 'percentage' : 'y';

    const options: uPlot.Options = {
      width: Math.floor(host.clientWidth),
      height,
      padding: [8, 8, 0, 0],
      cursor: { y: false, drag: { x: false, y: false }, points: { show: false } },
      legend: { show: false },
      scales: {
        x: { time: true, auto: false },
        [valueScaleKey]: scale === 'percentage'
          ? { auto: false, range: [0, 100] }
          : { auto: true }
      },
      axes: [
        {
          stroke: axis,
          grid: { stroke: grid, width: 1 },
          ticks: { stroke: grid, width: 1 },
          font: `500 10px ${chartFont}`,
          size: 28,
          values: (_plot, ticks) => ticks.map((value) => axisTimeFormatter.format(value * 1_000))
        },
        {
          scale: valueScaleKey,
          stroke: axis,
          grid: { stroke: grid, width: 1 },
          ticks: { stroke: grid, width: 1 },
          font: `500 10px ${chartFont}`,
          size: 54,
          values: scale === 'percentage'
            ? (_plot, ticks) => ticks.map((value) => `${value.toFixed(0)}%`)
            : undefined
        }
      ],
      series: [
        {},
        ...series.map((item, index) => ({
          label: item.label,
          scale: valueScaleKey,
          stroke: item.color ?? palette[index % palette.length],
          fill: index === 0 ? fills[index % fills.length] : undefined,
          width: index === 0 ? 2 : 1.5,
          dash: index === 0 ? [] : [6, 4],
          points: { show: false }
        }))
      ],
      hooks: {
        drawClear: [(plot) => shadeGaps(plot, missingIntervals)],
        setCursor: [(plot) => {
          const index = plot.cursor.idx;
          if (index == null) {
            hovered = null;
            return;
          }
          const timestamp = plot.data[0][index];
          if (timestamp == null) return;
          const timeMs = timestamp * 1_000;
          hovered = {
            timeMs,
            values: series.map((_item, seriesIndex) => plot.data[seriesIndex + 1][index] ?? null)
          };
          onSelectedTimeChange(timeMs);
        }]
      }
    };
    chart = new UPlot(options, data, host);
    renderedScale = scale;
    renderedWindowDurationMs = windowDurationMs;
    chart.setScale('x', visibleScale());
    synchronizeSelection(selectedTimeMs);
  }

  function updateChart(): void {
    if (!UPlot || !host || host.clientWidth === 0) return;
    if (!hasData) {
      chart?.destroy();
      chart = undefined;
      return;
    }
    if (!chart || chart.series.length !== series.length + 1 || renderedScale !== scale) {
      buildChart();
      return;
    }

    const width = Math.floor(host.clientWidth);
    if (chart.width !== width || chart.height !== height) chart.setSize({ width, height });
    chart.setData(alignedData(series, missingIntervals), false);
    const windowChanged = renderedWindowDurationMs !== windowDurationMs;
    animateVisibleScale(windowChanged || followingLatest);
    renderedWindowDurationMs = windowDurationMs;
    synchronizeSelection(selectedTimeMs);
    if (!reducedMotionQuery?.matches) {
      chart.root.animate(
        [{ opacity: 0.88 }, { opacity: 1 }],
        { duration: 220, easing: 'cubic-bezier(0.16, 1, 0.3, 1)' }
      );
    }
  }

  function animateVisibleScale(animate: boolean): void {
    if (!chart) return;
    const target = visibleScale();
    const fromMin = chart.scales.x.min;
    const fromMax = chart.scales.x.max;
    if (scaleAnimationFrame != null) {
      cancelAnimationFrame(scaleAnimationFrame);
      scaleAnimationFrame = undefined;
    }
    if (
      reducedMotionQuery?.matches
      || fromMin == null
      || fromMax == null
      || !animate
    ) {
      chart.setScale('x', target);
      return;
    }

    const startedAt = performance.now();
    const duration = 420;
    const frame = (now: number) => {
      if (!chart) return;
      const progress = Math.min(1, (now - startedAt) / duration);
      const eased = 1 - (1 - progress) ** 4;
      chart.setScale('x', {
        min: fromMin + (target.min - fromMin) * eased,
        max: fromMax + (target.max - fromMax) * eased
      });
      if (progress < 1) scaleAnimationFrame = requestAnimationFrame(frame);
      else scaleAnimationFrame = undefined;
    };
    scaleAnimationFrame = requestAnimationFrame(frame);
  }

  function panByWheel(event: WheelEvent): void {
    if (event.ctrlKey) return;
    const useShiftedVertical = event.shiftKey && Math.abs(event.deltaY) > Math.abs(event.deltaX);
    if (!useShiftedVertical && Math.abs(event.deltaX) <= Math.abs(event.deltaY)) return;

    const delta = useShiftedVertical ? event.deltaY : event.deltaX;
    if (delta === 0) return;
    if (event.cancelable) event.preventDefault();

    const nextEndMs = clampChartWindowEnd(
      visibleEnd() + wheelDeltaToTimeMs(delta, event.deltaMode, host.clientWidth, windowDurationMs),
      earliestSampleMs,
      serverTimeMs,
      windowDurationMs
    );
    onViewportChange(nextEndMs, Math.abs(serverTimeMs - nextEndMs) < 1_000);
  }

  function showLatest(): void {
    onViewportChange(serverTimeMs, true);
  }

  $effect(() => {
    series;
    sampleTimesMs;
    serverTimeMs;
    online;
    scale;
    height;
    windowDurationMs;
    viewEndMs;
    followingLatest;
    if (UPlot) queueMicrotask(updateChart);
  });

  $effect(() => {
    selectedTimeMs;
    if (chart) queueMicrotask(() => synchronizeSelection(selectedTimeMs));
  });

  onMount(() => {
    let active = true;
    const observer = new ResizeObserver(() => {
      const width = Math.floor(host.clientWidth);
      if (chart && width > 0 && (chart.width !== width || chart.height !== height)) {
        chart.setSize({ width, height });
      }
    });
    observer.observe(host);

    const leave = () => {
      hovered = null;
      chart?.setCursor({ left: -10, top: -10 }, false);
      onSelectedTimeChange(null);
    };
    host.addEventListener('mouseleave', leave);
    host.addEventListener('wheel', panByWheel, { passive: false });
    host.addEventListener('dblclick', onToggleWindow);

    schemeQuery = window.matchMedia('(prefers-color-scheme: dark)');
    reducedMotionQuery = window.matchMedia('(prefers-reduced-motion: reduce)');
    const redrawForScheme = () => queueMicrotask(buildChart);
    schemeQuery.addEventListener('change', redrawForScheme);

    void import('uplot').then((module) => {
      if (!active) return;
      UPlot = module.default;
      buildChart();
    });

    return () => {
      active = false;
      observer.disconnect();
      host.removeEventListener('mouseleave', leave);
      host.removeEventListener('wheel', panByWheel);
      host.removeEventListener('dblclick', onToggleWindow);
      schemeQuery?.removeEventListener('change', redrawForScheme);
      if (scaleAnimationFrame != null) cancelAnimationFrame(scaleAnimationFrame);
      chart?.destroy();
    };
  });
</script>

<section
  class="chart-card"
  class:offline={!online}
  aria-label={`${title} history`}
  data-window-hours={windowDurationMs / 3_600_000}
>
  <header>
    <div class="chart-title">
      <span>{title}</span>
      {#if subtitle}<small>{subtitle}</small>{/if}
    </div>
    <div class="readout" aria-live="off">
      <div class="values">
        {#each series as item, index (item.label)}
          <span><i class:secondary={index > 0}></i>{item.label} <strong>{shownValue(item, index)}</strong></span>
        {/each}
      </div>
      <time>{shownTime()}</time>
    </div>
  </header>
  <div class="chart-shell" class:is-empty={!hasData}>
    <div
      bind:this={host}
      class="chart"
      role="img"
      aria-label={`${title} time series. Shift-scroll to browse retained history. Double-click to toggle one hour and 24 hours.`}
    ></div>
    {#if !hasData}<p class="empty">{emptyLabel}</p>{/if}
  </div>
  <footer>
    {#if !followingLatest}<button type="button" onclick={showLatest}>Return to latest</button>{/if}
    <span class="gap-key" aria-hidden="true"><i></i>Missing or stale data</span>
  </footer>
</section>

<style>
  /* Hallmark · component: fixed-window telemetry chart · genre: modern-minimal · theme: Cobalt */
  .chart-card {
    min-width: 0;
    height: 16.625rem;
    overflow: hidden;
    border: var(--rule-hairline) solid var(--color-rule);
    border-radius: var(--radius-control);
    background: var(--color-paper);
  }
  .chart-card.offline { border-color: var(--color-danger-rule); }
  header {
    display: grid;
    height: 3.75rem;
    grid-template-columns: minmax(5.625rem, 0.55fr) minmax(0, 1fr);
    align-items: start;
    gap: var(--space-sm);
    padding: var(--space-sm) var(--space-sm) var(--space-2xs);
  }
  .chart-title { display: flex; min-width: 0; flex-direction: column; gap: var(--space-3xs); }
  .chart-title > span { overflow: hidden; color: var(--color-ink); font-family: var(--font-display); font-size: var(--text-sm); font-weight: 650; text-overflow: ellipsis; white-space: nowrap; }
  .chart-title small, time { color: var(--color-muted); font-family: var(--font-mono); font-size: var(--text-xs); font-weight: 450; }
  .readout { display: flex; min-width: 0; flex-direction: column; align-items: flex-end; gap: var(--space-3xs); text-align: right; }
  .values { display: flex; min-height: 1rem; flex-wrap: wrap; align-content: start; justify-content: flex-end; gap: var(--space-3xs) var(--space-sm); }
  .values span { display: inline-flex; align-items: center; gap: var(--space-2xs); color: var(--color-muted); font-family: var(--font-mono); font-size: var(--text-xs); white-space: nowrap; }
  .values i { width: var(--space-xs); height: var(--rule-strong); border-radius: var(--radius-mark); background: var(--color-chart-1); }
  .values i.secondary { background: var(--color-chart-2); }
  .values strong { color: var(--color-ink); font-size: var(--text-xs); font-variant-numeric: tabular-nums; font-weight: 650; }
  time { min-height: 1em; font-variant-numeric: tabular-nums; }
  .chart-shell { position: relative; height: 11rem; overflow: hidden; border-top: var(--rule-hairline) solid var(--color-paper-3); }
  .chart-shell.is-empty { display: grid; place-items: center; }
  .chart { width: 100%; height: 11rem; touch-action: pan-y; user-select: none; }
  .empty { position: absolute; inset: 0; display: grid; place-items: center; margin: 0; color: var(--color-muted); font-family: var(--font-mono); font-size: var(--text-xs); }
  footer { display: flex; height: 1.875rem; align-items: center; justify-content: flex-end; gap: var(--space-xs); padding: 0 var(--space-sm); color: var(--color-muted); font-family: var(--font-mono); font-size: var(--text-xs); }
  footer span { display: inline-flex; align-items: center; gap: var(--space-2xs); }
  footer i { width: var(--space-xs); height: var(--space-xs); border: var(--rule-hairline) solid var(--color-danger-rule); border-radius: var(--radius-mark); background: var(--color-danger-surface); }
  footer button { border: 0; border-radius: var(--radius-mark); padding: var(--space-3xs) var(--space-2xs); background: transparent; color: var(--color-ink); font: inherit; cursor: pointer; white-space: nowrap; }
  footer button:active { background: var(--color-paper-3); }
  footer button:disabled { cursor: not-allowed; opacity: 0.55; }
  :global(.u-wrap) { font-family: var(--font-mono); }
  :global(.u-cursor-x), :global(.u-cursor-y) { border-color: var(--color-muted) !important; opacity: 0.6; }
  @media (hover: hover) and (pointer: fine) {
    footer button:hover { background: var(--color-paper-2); }
  }
  @media (max-width: 32.5rem) {
    header { height: 5.5rem; grid-template-columns: 1fr; gap: var(--space-3xs); overflow: hidden; }
    .chart-card { height: 18.375rem; }
    .chart-shell, .chart { height: 11rem; }
    .readout { text-align: left; }
    .values { justify-content: flex-start; }
    .gap-key { display: none; }
  }
</style>
