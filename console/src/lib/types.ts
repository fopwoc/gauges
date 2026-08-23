export interface ProbeIdentity {
  hostname: string;
  distro: string;
  distroVersion: string;
  kernelVersion: string;
  ipAddress: string | null;
  gpuTypes: string[];
}

export interface CpuMetric {
  usagePercent: number;
  temperatureCelsius: number | null;
}

export interface MemoryMetric {
  totalBytes: number;
  usedBytes: number;
  availableBytes: number;
  swapTotalBytes: number;
  swapUsedBytes: number;
}

export interface DiskMetric {
  device: string;
  mountPoint: string;
  fileSystem: string;
  totalBytes: number;
  usedBytes: number;
  usagePercent: number;
}

export interface NetworkMetric {
  interface: string;
  receivedBytesPerSecond: number;
  transmittedBytesPerSecond: number;
  totalReceivedBytes: number;
  totalTransmittedBytes: number;
}

export interface GpuMetric {
  device: string;
  driver: string | null;
  usagePercent: number | null;
  temperatureCelsius: number | null;
  vramTotalBytes: number | null;
  vramUsedBytes: number | null;
}

export interface MetricSample {
  sampleId: string;
  capturedAtMs: number;
  cpu: CpuMetric;
  memory: MemoryMetric;
  disks: DiskMetric[];
  networks: NetworkMetric[];
  gpus: GpuMetric[];
  powerWatts: number | null;
  uptimeSeconds: number;
}

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

export interface ConsoleConfig {
  pollIntervalMs: number;
}

export interface ChartPoint {
  timeMs: number;
  value: number | null;
}

export interface ChartSeries {
  label: string;
  color?: string;
  points: ChartPoint[];
  format?: (value: number) => string;
}

export type ChartScale = 'automatic' | 'percentage';

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

export interface MissingInterval {
  startMs: number;
  endMs: number;
  kind: 'gap' | 'stale';
}
