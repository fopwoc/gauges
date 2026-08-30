import type { DeviceSummary, MetricSample } from './protocol';

export interface ConsoleConfig {
  pollIntervalMs: number;
}

export interface DeviceHistoryState {
  samples: MetricSample[];
  retentionHours: number | null;
  collectionIntervalSeconds: number | null;
  lastMetricTimeMs: number | null;
  initialized: boolean;
  loading: boolean;
  error: string | null;
}

export interface DeviceSummaryState {
  device: DeviceSummary | null;
  initialized: boolean;
  loading: boolean;
  error: string | null;
}
