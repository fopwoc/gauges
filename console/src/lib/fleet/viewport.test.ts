import { describe, expect, test } from 'bun:test';
import { overscanDeviceIds } from './viewport';

describe('viewport overscan', () => {
  const devices = ['a', 'b', 'c', 'd', 'e', 'f', 'g'];

  test('keeps two machines before and after the visible range', () => {
    expect(overscanDeviceIds(devices, new Set(['c', 'd']))).toEqual(['a', 'b', 'c', 'd', 'e', 'f']);
  });

  test('clamps the range to the directory', () => {
    expect(overscanDeviceIds(devices, new Set(['a']))).toEqual(['a', 'b', 'c']);
    expect(overscanDeviceIds(devices, new Set(['g']))).toEqual(['e', 'f', 'g']);
  });

  test('ignores stale visibility entries', () => {
    expect(overscanDeviceIds(devices, new Set(['removed']))).toEqual([]);
  });
});
