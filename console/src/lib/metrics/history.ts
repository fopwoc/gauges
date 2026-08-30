import type { ChartPoint, MetricSample, MissingInterval } from '../types';

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
