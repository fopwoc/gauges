import type { ProbeIdentity } from './identity';
import type { MetricSample } from './metrics';

export interface DeviceSummary {
  deviceId: string;
  displayName: string;
  identity: ProbeIdentity;
  retentionHours: number;
  collectionIntervalSeconds: number;
  lastMetricTimeMs: number;
  latestSample: MetricSample | null;
  online: boolean;
}

export interface DeviceIndexResponse {
  revision: string;
  total: number;
  online: number;
  offline: number;
  lastMetricTimeMs: number | null;
  deviceIds: string[];
}

export interface DeviceSummarySyncResponse {
  serverTimeMs: number;
  devices: Record<string, DeviceSummary | null>;
}
