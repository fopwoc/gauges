import { describe, expect, test } from 'bun:test';
import {
  clampChartWindowEnd,
  findMissingIntervals,
  formatBytes,
  formatCompactNumber,
  formatCpuModel,
  formatDuration,
  formatRate,
  mergeSamples,
  relativeTime,
  summarizeStorage,
  valueAtLatestSample,
  wheelDeltaToTimeMs
} from './metrics';
import type { MetricSample } from './types';

function sample(sampleId: string, capturedAtMs: number): MetricSample {
  return {
    sampleId,
    capturedAtMs,
    cpu: { usagePercent: 0, temperatureCelsius: null },
    memory: { totalBytes: 0, usedBytes: 0, availableBytes: 0, swapTotalBytes: 0, swapUsedBytes: 0 },
    storage: [],
    networks: [],
    gpus: [],
    powerWatts: null,
    uptimeSeconds: 0
  };
}

describe('metric helpers', () => {
  test('formats compact values', () => {
    expect(formatBytes(1_073_741_824)).toBe('1.00 GiB');
    expect(formatDuration(93_720)).toBe('1d 2h');
    expect(formatRate(1_000_000)).toBe('8 Mb/s');
    expect(formatCompactNumber(10_000)).toBe('10k');
    expect(formatCompactNumber(1_000_000)).toBe('1m');
  });

  test('formats CPU names for compact card headers', () => {
    expect(formatCpuModel('AMD Ryzen 7 5700X 8-Core Processor')).toBe('Ryzen 7 5700X');
    expect(formatCpuModel('Intel(R) Core(TM) i5-12400 CPU @ 2.50GHz')).toBe('Intel Core i5-12400');
    expect(formatCpuModel('AMD Ryzen 5 5600G with Radeon Graphics')).toBe('Ryzen 5 5600G');
    expect(formatCpuModel(null)).toBeUndefined();
  });

  test('summarizes storage pools and identifies the fullest pool', () => {
    const summary = summarizeStorage([
      {
        kind: 'disk', id: 'disk:a', label: 'a', device: 'a', fileSystems: ['ext4'], mountPoints: ['/'],
        totalBytes: 100, usedBytes: 40, usagePercent: 40, fullestFilesystem: null
      },
      {
        kind: 'disk', id: 'disk:b', label: 'archive', device: 'b', fileSystems: ['xfs'], mountPoints: ['/archive'],
        totalBytes: 300, usedBytes: 270, usagePercent: 90, fullestFilesystem: null
      }
    ]);
    expect(summary).toMatchObject({
      totalBytes: 400,
      usedBytes: 310,
      usagePercent: 77.5,
      poolCount: 2,
      fullest: { label: 'archive' }
    });
  });

  test('formats time relative to the supplied clock without correcting it', () => {
    expect(relativeTime(13_000, 10_000)).toBe('just now');
  });

  test('merges, deduplicates, trims, and sorts history', () => {
    const merged = mergeSamples(
      [sample('later', 30), sample('duplicate', 20), sample('expired', 5)],
      [sample('duplicate', 20), sample('new', 25)],
      10
    );
    expect(merged.map(({ sampleId }) => sampleId)).toEqual(['duplicate', 'new', 'later']);
  });

  test('uses the probe cadence for gap detection', () => {
    expect(findMissingIntervals([0, 10_000, 70_000], 80_000, 10_000, true)).toEqual([
      { startMs: 22_500, endMs: 67_500, kind: 'gap' }
    ]);
    expect(findMissingIntervals([0, 60_000], 65_000, 60_000, true)).toEqual([]);
  });

  test('current readout preserves null at the latest actual sample', () => {
    expect(valueAtLatestSample(
      [{ timeMs: 10_000, value: 42 }, { timeMs: 20_000, value: null }],
      [10_000, 20_000]
    )).toBeNull();
  });

  test('keeps a fixed chart window inside retained history', () => {
    const hour = 3_600_000;
    expect(clampChartWindowEnd(30 * 60_000, 0, 24 * hour, hour)).toBe(hour);
    expect(clampChartWindowEnd(12 * hour, 0, 24 * hour, hour)).toBe(12 * hour);
    expect(clampChartWindowEnd(25 * hour, 0, 24 * hour, hour)).toBe(24 * hour);
    expect(clampChartWindowEnd(0, 10 * 60_000, 40 * 60_000, hour)).toBe(40 * 60_000);
  });

  test('maps trackpad wheel distance onto the visible time window', () => {
    const hour = 3_600_000;
    expect(wheelDeltaToTimeMs(-100, 0, 600, hour)).toBe(-600_000);
    expect(wheelDeltaToTimeMs(2, 1, 640, hour)).toBe(180_000);
    expect(wheelDeltaToTimeMs(1, 2, 640, hour)).toBe(hour);
  });

  test('finds internal gaps and an offline trailing region', () => {
    const intervals = findMissingIntervals(
      [0, 10_000, 20_000, 80_000, 90_000],
      120_000,
      10_000,
      false
    );
    expect(intervals).toEqual([
      { startMs: 32_500, endMs: 77_500, kind: 'gap' },
      { startMs: 90_000, endMs: 120_000, kind: 'stale' }
    ]);
  });

  test('does not mark a healthy trailing cadence as stale', () => {
    expect(findMissingIntervals([0, 10_000, 20_000], 29_000, 10_000, true)).toEqual([]);
  });

  test('replayed samples fill a previously missing interval', () => {
    const original = [sample('0', 0), sample('10', 10_000), sample('70', 70_000), sample('80', 80_000)];
    expect(findMissingIntervals(original.map((item) => item.capturedAtMs), 89_000, 10_000, true)).toHaveLength(1);

    const replayed = [20_000, 30_000, 40_000, 50_000, 60_000]
      .map((timestamp) => sample(String(timestamp), timestamp));
    const complete = mergeSamples(original, replayed, 0);
    expect(findMissingIntervals(complete.map((item) => item.capturedAtMs), 89_000, 10_000, true)).toEqual([]);
  });
});
