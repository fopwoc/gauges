import type { MetricSample } from './metrics';

export interface MachineHistory {
  retentionHours: number;
  collectionIntervalSeconds: number;
  nextMetricTimeMs: number | null;
  samples: MetricSample[];
}

export interface HistorySyncResponse {
  serverTimeMs: number;
  machines: Record<string, MachineHistory>;
}
