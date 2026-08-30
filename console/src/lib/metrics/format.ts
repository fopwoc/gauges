const UNITS = ['B', 'KiB', 'MiB', 'GiB', 'TiB', 'PiB'];
const RATE_UNITS = ['b/s', 'Kb/s', 'Mb/s', 'Gb/s', 'Tb/s'];
const COMPACT_SUFFIXES = ['', 'k', 'm', 'b', 't'];

export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return '0 B';
  const exponent = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), UNITS.length - 1);
  const value = bytes / 1024 ** exponent;
  return `${value.toFixed(value >= 100 || exponent === 0 ? 0 : value >= 10 ? 1 : 2)} ${UNITS[exponent]}`;
}

export function formatCpuModel(value: string | null | undefined): string | undefined {
  if (!value?.trim()) return undefined;

  const formatted = value
    .replace(/\((?:R|TM)\)/gi, '')
    .replace(/\s+/g, ' ')
    .trim()
    .replace(/^AMD\s+/i, '')
    .replace(/\s+with Radeon Graphics$/i, '')
    .replace(/\s+\d+-Core Processor.*$/i, '')
    .replace(/\s+Processor$/i, '')
    .replace(/\s+@\s+.*$/i, '')
    .replace(/\s+CPU(?:\s+|$)/i, ' ')
    .replace(/\s+/g, ' ')
    .trim();

  return formatted || undefined;
}

export function formatRate(bytesPerSecond: number): string {
  return formatBitsPerSecond(bytesPerSecond * 8);
}

export function toBitsPerSecond(bytesPerSecond: number | null | undefined): number | null {
  return bytesPerSecond == null ? null : bytesPerSecond * 8;
}

export function formatBitsPerSecond(bitsPerSecond: number): string {
  if (!Number.isFinite(bitsPerSecond) || bitsPerSecond <= 0) return '0 b/s';
  const exponent = Math.min(
    Math.floor(Math.log(bitsPerSecond) / Math.log(1_000)),
    RATE_UNITS.length - 1
  );
  const value = bitsPerSecond / 1_000 ** exponent;
  return `${formatSignificantValue(value)} ${RATE_UNITS[exponent]}`;
}

export function formatCompactNumber(value: number): string {
  if (!Number.isFinite(value) || value === 0) return '0';
  const absolute = Math.abs(value);
  const exponent = Math.min(
    Math.floor(Math.log(absolute) / Math.log(1_000)),
    COMPACT_SUFFIXES.length - 1
  );
  const scaled = value / 1_000 ** Math.max(0, exponent);
  return `${formatSignificantValue(scaled)}${COMPACT_SUFFIXES[Math.max(0, exponent)]}`;
}

function formatSignificantValue(value: number): string {
  const absolute = Math.abs(value);
  const digits = absolute >= 100 ? 0 : absolute >= 10 ? 1 : 2;
  const formatted = value.toFixed(digits);
  return digits === 0 ? formatted : formatted.replace(/\.?0+$/, '');
}

export function formatPercent(value: number | null | undefined): string {
  return value == null || !Number.isFinite(value) ? '—' : `${value.toFixed(1)}%`;
}

export function formatTemperature(value: number | null | undefined): string {
  return value == null || !Number.isFinite(value) ? '—' : `${value.toFixed(1)}°C`;
}

export function formatDuration(totalSeconds: number): string {
  if (!Number.isFinite(totalSeconds) || totalSeconds < 0) return '—';
  const days = Math.floor(totalSeconds / 86_400);
  const hours = Math.floor((totalSeconds % 86_400) / 3_600);
  const minutes = Math.floor((totalSeconds % 3_600) / 60);
  if (days > 0) return `${days}d ${hours}h`;
  if (hours > 0) return `${hours}h ${minutes}m`;
  return `${minutes}m`;
}

export function usagePercent(used: number, total: number): number {
  return total > 0 ? Math.min(100, Math.max(0, (used / total) * 100)) : 0;
}

export function relativeTime(nowMs: number, timestampMs: number): string {
  const seconds = Math.max(0, Math.floor((nowMs - timestampMs) / 1_000));
  if (seconds < 5) return 'just now';
  if (seconds < 60) return `${seconds}s ago`;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h ago`;
  return `${Math.floor(hours / 24)}d ago`;
}
