<script lang="ts">
  let {
    label,
    value,
    primary,
    secondary,
    warningAt = 85,
    dangerAt = 95
  }: {
    label: string;
    value: number;
    primary: string;
    secondary?: string;
    warningAt?: number;
    dangerAt?: number;
  } = $props();

  let normalized = $derived(Math.min(100, Math.max(0, value)));
  let warning = $derived(normalized >= warningAt && normalized < dangerAt);
  let danger = $derived(normalized >= dangerAt);
</script>

<div
  class="capacity"
  class:warning
  class:danger
  role="meter"
  aria-label={label}
  aria-valuemin="0"
  aria-valuemax="100"
  aria-valuenow={normalized}
  aria-valuetext={`${normalized.toFixed(1)}%, ${primary}`}
>
  <div class="labels">
    <span>{label}</span>
    <strong>{primary}</strong>
  </div>
  <div class="track" aria-hidden="true"><i style:width={`${normalized}%`}></i></div>
  {#if secondary}<small>{secondary}</small>{/if}
</div>

<style>
  /* Hallmark · component: capacity bar · genre: modern-minimal · theme: Cobalt */
  .capacity { min-width: 0; color: var(--color-accent); }
  .capacity.warning { color: var(--color-warning); }
  .capacity.danger { color: var(--color-danger); }
  .labels { display: flex; min-width: 0; align-items: baseline; justify-content: space-between; gap: var(--space-sm); }
  .labels span { overflow: hidden; color: var(--color-ink); font-family: var(--font-display); font-size: var(--text-xs); font-weight: 650; text-overflow: ellipsis; white-space: nowrap; }
  .labels strong,
  small { color: var(--color-muted); font-family: var(--font-mono); font-size: var(--text-xs); font-variant-numeric: tabular-nums; font-weight: 520; }
  .track { height: 0.4375rem; overflow: hidden; margin-top: var(--space-2xs); border-radius: var(--radius-mark); background: var(--color-paper-3); }
  .track i { display: block; height: 100%; border-radius: inherit; background: currentColor; transition: width var(--dur-long) var(--ease-out); }
  small { display: block; overflow: hidden; margin-top: var(--space-2xs); text-overflow: ellipsis; white-space: nowrap; }
  @media (prefers-reduced-motion: reduce) { .track i { transition: none; } }
</style>

