import { mergeSamples } from '$lib/metrics';
import type {
  DeviceHistoryState,
  DeviceIndexResponse,
  DeviceSummaryState,
  DeviceSummarySyncResponse,
  HistorySyncResponse
} from '$lib/types';
import { overscanDeviceIds } from './viewport';

const MILLIS_PER_HOUR = 3_600_000;

const EMPTY_SUMMARY: DeviceSummaryState = {
  device: null,
  initialized: false,
  loading: false,
  error: null
};

const EMPTY_HISTORY: DeviceHistoryState = {
  samples: [],
  retentionHours: null,
  collectionIntervalSeconds: null,
  lastMetricTimeMs: null,
  initialized: false,
  loading: false,
  error: null
};

export class FleetSync {
  deviceIds = $state<string[]>([]);
  total = $state(0);
  online = $state(0);
  offline = $state(0);
  lastMetricTimeMs = $state<number | null>(null);
  revision = $state('');
  summaries = $state<Record<string, DeviceSummaryState>>({});
  histories = $state<Record<string, DeviceHistoryState>>({});
  expandedIds = $state<Set<string>>(new Set());
  serverTimeMs = $state(Date.now());
  serverTimeObservedAtMs = $state<number | null>(null);
  directoryLoading = $state(true);
  syncing = $state(false);
  hubError = $state<string | null>(null);

  readonly forceExpanded: boolean;
  readonly pollIntervalMs: number;

  #active = false;
  #batching = false;
  #batchAgain = false;
  #batchQueued = false;
  #pollTimer: number | undefined;
  #controller = new AbortController();
  #visibleIds = new Set<string>();
  #summaryInterest = new Set<string>();
  #indexError: string | null = null;
  #summaryError: string | null = null;
  #historyError: string | null = null;

  constructor(options: {
    forceExpanded: boolean;
    pollIntervalMs: number;
  }) {
    this.forceExpanded = options.forceExpanded;
    this.pollIntervalMs = options.pollIntervalMs;
  }

  start(): () => void {
    this.#active = true;
    void this.refreshNow();
    this.#pollTimer = window.setInterval(() => {
      if (!document.hidden) void this.refreshNow();
    }, this.pollIntervalMs);
    const resume = () => {
      if (!document.hidden) void this.refreshNow();
    };
    document.addEventListener('visibilitychange', resume);

    return () => {
      this.#active = false;
      this.#controller.abort();
      if (this.#pollTimer != null) window.clearInterval(this.#pollTimer);
      document.removeEventListener('visibilitychange', resume);
    };
  }

  summaryState(deviceId: string): DeviceSummaryState {
    return this.summaries[deviceId] ?? EMPTY_SUMMARY;
  }

  historyState(deviceId: string): DeviceHistoryState {
    return this.histories[deviceId] ?? EMPTY_HISTORY;
  }

  isExpanded(deviceId: string): boolean {
    return this.forceExpanded || this.expandedIds.has(deviceId);
  }

  isSubscribed(deviceId: string): boolean {
    return this.#summaryInterest.has(deviceId);
  }

  setExpanded(deviceId: string, expanded: boolean): void {
    if (this.forceExpanded) return;
    const next = new Set(this.expandedIds);
    if (expanded) next.add(deviceId);
    else next.delete(deviceId);
    this.expandedIds = next;
    this.#reconcileInterest();
  }

  setVisibleDeviceIds(deviceIds: ReadonlySet<string>): void {
    this.#visibleIds = new Set(deviceIds);
    this.#reconcileInterest();
  }

  async refreshNow(): Promise<void> {
    if (!this.#active) return;
    await this.#refreshIndex();
    this.#queueBatch();
  }

  #reconcileInterest(): void {
    this.#summaryInterest = new Set(overscanDeviceIds(this.deviceIds, this.#visibleIds));

    const summaries: Record<string, DeviceSummaryState> = {};
    for (const deviceId of this.#summaryInterest) {
      const current = this.summaries[deviceId];
      if (current) summaries[deviceId] = current;
    }
    this.summaries = summaries;

    const histories: Record<string, DeviceHistoryState> = {};
    for (const deviceId of this.#summaryInterest) {
      if (!this.isExpanded(deviceId)) continue;
      const current = this.histories[deviceId];
      if (current) histories[deviceId] = current;
    }
    this.histories = histories;
    this.#queueBatch();
  }

  #queueBatch(): void {
    if (!this.#active || this.#batchQueued) return;
    this.#batchQueued = true;
    queueMicrotask(() => {
      this.#batchQueued = false;
      void this.#syncInterested();
    });
  }

  async #refreshIndex(): Promise<void> {
    try {
      const response = await this.#jsonRequest<DeviceIndexResponse>('/api/v1/devices/index');
      if (!this.#active) return;
      const changed = response.revision !== this.revision;
      this.revision = response.revision;
      this.total = response.total;
      this.online = response.online;
      this.offline = response.offline;
      this.lastMetricTimeMs = response.lastMetricTimeMs;
      if (changed || this.deviceIds.length !== response.deviceIds.length) {
        this.deviceIds = response.deviceIds;
        const activeIds = new Set(response.deviceIds);
        this.expandedIds = new Set([...this.expandedIds].filter((id) => activeIds.has(id)));
        this.#visibleIds = new Set([...this.#visibleIds].filter((id) => activeIds.has(id)));
        this.#reconcileInterest();
      }
      this.#indexError = null;
      this.#updateHubError();
    } catch (cause) {
      if (!this.#controller.signal.aborted) {
        this.#indexError = errorMessage(cause);
        this.#updateHubError();
      }
    } finally {
      this.directoryLoading = false;
    }
  }

  async #syncInterested(): Promise<void> {
    if (!this.#active) return;
    if (this.#batching) {
      this.#batchAgain = true;
      return;
    }

    this.#batching = true;
    this.syncing = true;
    do {
      this.#batchAgain = false;
      const summaryIds = [...this.#summaryInterest];
      const historyIds = summaryIds.filter((deviceId) => this.isExpanded(deviceId));
      await Promise.all([
        this.#syncSummaries(summaryIds),
        this.#syncHistories(historyIds)
      ]);
    } while (this.#active && this.#batchAgain);
    this.#batching = false;
    this.syncing = false;
  }

  async #syncSummaries(deviceIds: string[]): Promise<void> {
    if (deviceIds.length === 0) {
      this.#summaryError = null;
      this.#updateHubError();
      return;
    }
    for (const deviceId of deviceIds) {
      const current = this.summaries[deviceId] ?? EMPTY_SUMMARY;
      this.#setSummary(deviceId, { ...current, loading: !current.initialized, error: null });
    }

    try {
      const response = await this.#jsonRequest<DeviceSummarySyncResponse>('/api/v1/devices/sync', {
        method: 'POST',
        body: JSON.stringify({ deviceIds })
      });
      if (!this.#active) return;
      this.#observeServerTime(response.serverTimeMs);
      for (const deviceId of deviceIds) {
        if (!this.#summaryInterest.has(deviceId) || !(deviceId in response.devices)) continue;
        this.#setSummary(deviceId, {
          device: response.devices[deviceId],
          initialized: true,
          loading: false,
          error: null
        });
      }
      this.#summaryError = null;
      this.#updateHubError();
    } catch (cause) {
      if (this.#controller.signal.aborted) return;
      const message = errorMessage(cause);
      this.#summaryError = message;
      this.#updateHubError();
      for (const deviceId of deviceIds) {
        if (!this.#summaryInterest.has(deviceId)) continue;
        const current = this.summaries[deviceId] ?? EMPTY_SUMMARY;
        this.#setSummary(deviceId, { ...current, loading: false, error: message });
      }
    }
  }

  async #syncHistories(deviceIds: string[]): Promise<void> {
    if (deviceIds.length === 0) {
      this.#historyError = null;
      this.#updateHubError();
      return;
    }
    const machines: Record<string, number | null> = {};
    for (const deviceId of deviceIds) {
      const current = this.histories[deviceId] ?? EMPTY_HISTORY;
      machines[deviceId] = current.initialized && current.samples.length > 0
        ? current.lastMetricTimeMs
        : null;
      this.#setHistory(deviceId, { ...current, loading: !current.initialized, error: null });
    }

    try {
      const response = await this.#jsonRequest<HistorySyncResponse>('/api/v1/history/sync', {
        method: 'POST',
        body: JSON.stringify({ machines })
      });
      if (!this.#active) return;
      this.#observeServerTime(response.serverTimeMs);
      for (const deviceId of deviceIds) {
        if (!this.#summaryInterest.has(deviceId) || !this.isExpanded(deviceId)) continue;
        const incoming = response.machines[deviceId];
        if (!incoming) continue;
        const current = this.histories[deviceId] ?? EMPTY_HISTORY;
        const initial = machines[deviceId] == null;
        const oldestAllowedMs = incoming.nextMetricTimeMs == null
          ? Number.NEGATIVE_INFINITY
          : incoming.nextMetricTimeMs - incoming.retentionHours * MILLIS_PER_HOUR;
        this.#setHistory(deviceId, {
          samples: mergeSamples(initial ? [] : current.samples, incoming.samples, oldestAllowedMs),
          retentionHours: incoming.retentionHours,
          collectionIntervalSeconds: incoming.collectionIntervalSeconds,
          lastMetricTimeMs: incoming.nextMetricTimeMs,
          initialized: true,
          loading: false,
          error: null
        });
      }
      this.#historyError = null;
      this.#updateHubError();
    } catch (cause) {
      if (this.#controller.signal.aborted) return;
      const message = errorMessage(cause);
      this.#historyError = message;
      this.#updateHubError();
      for (const deviceId of deviceIds) {
        if (!this.#summaryInterest.has(deviceId) || !this.isExpanded(deviceId)) continue;
        const current = this.histories[deviceId] ?? EMPTY_HISTORY;
        this.#setHistory(deviceId, { ...current, loading: false, error: message });
      }
    }
  }

  #setSummary(deviceId: string, state: DeviceSummaryState): void {
    this.summaries = { ...this.summaries, [deviceId]: state };
  }

  #setHistory(deviceId: string, state: DeviceHistoryState): void {
    this.histories = { ...this.histories, [deviceId]: state };
  }

  #observeServerTime(serverTimeMs: number): void {
    if (this.serverTimeObservedAtMs != null && serverTimeMs <= this.serverTimeMs) return;
    this.serverTimeMs = serverTimeMs;
    this.serverTimeObservedAtMs = Date.now();
  }

  #updateHubError(): void {
    this.hubError = this.#indexError ?? this.#summaryError ?? this.#historyError;
  }

  async #jsonRequest<T>(path: string, init?: RequestInit): Promise<T> {
    const response = await fetch(path, {
      ...init,
      headers: {
        accept: 'application/json',
        ...(init?.body ? { 'content-type': 'application/json' } : {})
      },
      signal: this.#controller.signal
    });
    if (!response.ok) {
      const body = await response.json().catch(() => null) as { message?: string } | null;
      throw new Error(body?.message ?? `${response.status} ${response.statusText}`);
    }
    return response.json() as Promise<T>;
  }
}

function errorMessage(cause: unknown): string {
  return cause instanceof Error ? cause.message : 'Could not reach the hub';
}
