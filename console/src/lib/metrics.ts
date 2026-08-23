import type { ChartPoint, DiskMetric, MetricSample, MissingInterval } from './types';

const UNITS = ['B', 'KiB', 'MiB', 'GiB', 'TiB', 'PiB'];

export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return '0 B';
  const exponent = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), UNITS.length - 1);
  const value = bytes / 1024 ** exponent;
  return `${value.toFixed(value >= 100 || exponent === 0 ? 0 : value >= 10 ? 1 : 2)} ${UNITS[exponent]}`;
}

export function formatRate(bytesPerSecond: number): string {
  return `${formatBytes(bytesPerSecond)}/s`;
}

export function formatPercent(value: number | null | undefined): string {
  return value == null || !Number.isFinite(value) ? '—' : `${value.toFixed(1)}%`;
}

export function formatTemperature(value: number | null | undefined): string {
  return value == null || !Number.isFinite(value) ? '—' : `${value.toFixed(1)}°C`;
}

export function formatDuration(totalSeconds: number): string {
  if (!Number.isFinite(totalSeconds) || totalSeconds < 0) return '—';
  const days = Math.floor(totalSeconds / 86_400);
  const hours = Math.floor((totalSeconds % 86_400) / 3_600);
  const minutes = Math.floor((totalSeconds % 3_600) / 60);
  if (days > 0) return `${days}d ${hours}h`;
  if (hours > 0) return `${hours}h ${minutes}m`;
  return `${minutes}m`;
}

export function usagePercent(used: number, total: number): number {
  return total > 0 ? Math.min(100, Math.max(0, (used / total) * 100)) : 0;
}

export interface StorageSummary {
  totalBytes: number;
  usedBytes: number;
  usagePercent: number;
  volumeCount: number;
  fullest: DiskMetric;
}

export function summarizeStorage(disks: DiskMetric[]): StorageSummary | null {
  if (disks.length === 0) return null;
  const totalBytes = disks.reduce((sum, disk) => sum + disk.totalBytes, 0);
  const usedBytes = disks.reduce((sum, disk) => sum + disk.usedBytes, 0);
  const fullest = disks.reduce((current, disk) =>
    disk.usagePercent > current.usagePercent ? disk : current
  );
  return {
    totalBytes,
    usedBytes,
    usagePercent: usagePercent(usedBytes, totalBytes),
    volumeCount: disks.length,
    fullest
  };
}

export function relativeTime(nowMs: number, timestampMs: number): string {
  const seconds = Math.max(0, Math.floor((nowMs - timestampMs) / 1_000));
  if (seconds < 5) return 'just now';
  if (seconds < 60) return `${seconds}s ago`;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h ago`;
  return `${Math.floor(hours / 24)}d ago`;
}

export function mergeSamples(
  existing: MetricSample[],
  incoming: MetricSample[],
  oldestAllowedMs: number
): MetricSample[] {
  const byId = new Map<string, MetricSample>();
  for (const sample of existing) {
    if (sample.capturedAtMs >= oldestAllowedMs) byId.set(sample.sampleId, sample);
  }
  for (const sample of incoming) {
    if (sample.capturedAtMs >= oldestAllowedMs) byId.set(sample.sampleId, sample);
  }
  return [...byId.values()].sort((a, b) => a.capturedAtMs - b.capturedAtMs);
}

export function valueAtLatestSample(points: ChartPoint[], sampleTimesMs: number[]): number | null {
  if (sampleTimesMs.length === 0) return null;
  let latestTimeMs = Number.NEGATIVE_INFINITY;
  for (const timestamp of sampleTimesMs) latestTimeMs = Math.max(latestTimeMs, timestamp);
  return points.findLast((point) => point.timeMs === latestTimeMs)?.value ?? null;
}

export function clampChartWindowEnd(
  requestedEndMs: number,
  earliestSampleMs: number,
  latestAllowedMs: number,
  windowMs: number
): number {
  if (!Number.isFinite(latestAllowedMs)) return requestedEndMs;
  const minimumEndMs = Number.isFinite(earliestSampleMs)
    ? Math.min(latestAllowedMs, earliestSampleMs + windowMs)
    : latestAllowedMs;
  return Math.min(latestAllowedMs, Math.max(minimumEndMs, requestedEndMs));
}

export function wheelDeltaToTimeMs(
  delta: number,
  deltaMode: number,
  viewportWidth: number,
  windowMs: number
): number {
  const width = Math.max(1, viewportWidth);
  const pixels = deltaMode === 1
    ? delta * 16
    : deltaMode === 2
      ? delta * width
      : delta;
  return (pixels / width) * windowMs;
}

export function findMissingIntervals(
  timestampsMs: number[],
  serverTimeMs: number,
  expectedCadenceMs: number,
  online: boolean
): MissingInterval[] {
  const sorted = [...new Set(timestampsMs)]
    .filter((timestamp) => Number.isFinite(timestamp))
    .sort((a, b) => a - b);
  if (sorted.length === 0) return [];

  const cadenceMs = Math.max(1_000, expectedCadenceMs);
  const staleThresholdMs = cadenceMs * 2.5;
  const intervals: MissingInterval[] = [];

  for (let index = 1; index < sorted.length; index += 1) {
    const previous = sorted[index - 1];
    const current = sorted[index];
    if (current - previous <= staleThresholdMs) continue;
    const startMs = previous + cadenceMs * 1.25;
    const endMs = current - cadenceMs * 0.25;
    if (endMs > startMs) intervals.push({ startMs, endMs, kind: 'gap' });
  }

  const latestMs = sorted[sorted.length - 1];
  const ageMs = serverTimeMs - latestMs;
  if (ageMs > 0 && !online) {
    intervals.push({ startMs: latestMs, endMs: serverTimeMs, kind: 'stale' });
  } else if (ageMs > staleThresholdMs) {
    intervals.push({
      startMs: latestMs + cadenceMs * 1.25,
      endMs: serverTimeMs,
      kind: 'stale'
    });
  }

  return intervals;
}
