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
