import type { CpuMetric, GpuMetric, MemoryMetric, NetworkMetric } from './system';
import type { StorageMetric } from './storage';

export interface MetricSample {
  sampleId: string;
  capturedAtMs: number;
  cpu: CpuMetric;
  memory: MemoryMetric;
  storage: StorageMetric[];
  networks: NetworkMetric[];
  gpus: GpuMetric[];
  powerWatts: number | null;
  uptimeSeconds: number;
}
