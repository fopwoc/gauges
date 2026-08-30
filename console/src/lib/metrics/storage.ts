import type { FilesystemConstraintMetric, StorageMetric } from '../types';
import { usagePercent } from './format';

export interface StorageSummary {
  totalBytes: number;
  usedBytes: number;
  usagePercent: number;
  poolCount: number;
  fullest: StorageMetric;
  fullestFilesystem: FilesystemConstraintMetric | null;
}

export function summarizeStorage(storage: StorageMetric[]): StorageSummary | null {
  if (storage.length === 0) return null;
  const totalBytes = storage.reduce((sum, pool) => sum + pool.totalBytes, 0);
  const usedBytes = storage.reduce((sum, pool) => sum + pool.usedBytes, 0);
  const fullest = storage.reduce((current, pool) =>
    pool.usagePercent > current.usagePercent ? pool : current
  );
  const fullestFilesystem = storage
    .flatMap((pool) => pool.fullestFilesystem ? [pool.fullestFilesystem] : [])
    .reduce<FilesystemConstraintMetric | null>(
      (current, filesystem) =>
        current == null || filesystem.usagePercent > current.usagePercent ? filesystem : current,
      null
    );
  return {
    totalBytes,
    usedBytes,
    usagePercent: usagePercent(usedBytes, totalBytes),
    poolCount: storage.length,
    fullest,
    fullestFilesystem
  };
}
