import type { CpuMetric, GpuMetric, MemoryMetric, NetworkMetric } from './system';
import type { DriveTemperatureMetric, StorageMetric } from './storage';

export interface MetricSample {
  sampleId: string;
  capturedAtMs: number;
  cpu: CpuMetric;
  memory: MemoryMetric;
  storage: StorageMetric[];
  driveTemperatures?: DriveTemperatureMetric[];
  networks: NetworkMetric[];
  gpus: GpuMetric[];
  powerWatts: number | null;
  uptimeSeconds: number;
}
