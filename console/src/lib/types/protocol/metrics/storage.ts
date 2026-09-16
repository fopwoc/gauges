export interface FilesystemConstraintMetric {
  mountPoint: string;
  fileSystem: string;
  totalBytes: number;
  usedBytes: number;
  usagePercent: number;
}

export interface StorageCommon {
  id: string;
  label: string;
  mountPoints: string[];
  totalBytes: number;
  usedBytes: number;
  usagePercent: number;
  fullestFilesystem: FilesystemConstraintMetric | null;
}

export interface DriveTemperatureMetric {
  device: string;
  temperatureCelsius: number;
}

export type DiskStorageMetric = StorageCommon & {
  kind: 'disk';
  device: string;
  fileSystems: string[];
};

export type FilesystemStorageMetric = StorageCommon & {
  kind: 'filesystem';
  source: string;
  fileSystem: string;
};

export interface BtrfsDeviceErrorMetric {
  read: number;
  write: number;
  flush: number;
  corruption: number;
  generation: number;
}

export interface SmartDeviceHealthMetric {
  device: string;
  status: 'pending' | 'passed' | 'failed' | 'standby' | 'unavailable';
  checkedAtMs: number | null;
}

export type BtrfsStorageMetric = StorageCommon & {
  kind: 'btrfs';
  uuid: string;
  devices: string[];
  dataProfiles: string[];
  metadataProfiles: string[];
  logicalBytes: number;
  physicalBytes: number;
  allocatedBytes: number | null;
  allocationUsedBytes: number | null;
  deviceErrors: BtrfsDeviceErrorMetric | null;
  smartHealth?: SmartDeviceHealthMetric[];
};

export interface MtdHealthMetric {
  correctedBits: number | null;
  eccFailures: number | null;
  badBlocks: number | null;
  reservedBadBlocks: number | null;
  bitflipThreshold: number | null;
}

export type UbiStorageMetric = StorageCommon & {
  kind: 'ubi';
  ubiDevice: string;
  volume: string;
  mtdDevice: string | null;
  volumeBytes: number | null;
  totalPebs: number | null;
  availablePebs: number | null;
  badPebs: number | null;
  reservedForBadPebs: number | null;
  maxEraseCount: number | null;
  readOnly: boolean | null;
  corrupted: boolean | null;
  mtdHealth: MtdHealthMetric | null;
};

export type StorageMetric =
  | DiskStorageMetric
  | FilesystemStorageMetric
  | BtrfsStorageMetric
  | UbiStorageMetric;
