export function overscanDeviceIds(
  deviceIds: string[],
  visibleIds: ReadonlySet<string>,
  overscan = 2
): string[] {
  if (deviceIds.length === 0 || visibleIds.size === 0) return [];

  let first = deviceIds.length;
  let last = -1;
  for (let index = 0; index < deviceIds.length; index += 1) {
    if (!visibleIds.has(deviceIds[index])) continue;
    first = Math.min(first, index);
    last = Math.max(last, index);
  }
  if (last < 0) return [];

  const padding = Math.max(0, Math.floor(overscan));
  return deviceIds.slice(Math.max(0, first - padding), Math.min(deviceIds.length, last + padding + 1));
}

