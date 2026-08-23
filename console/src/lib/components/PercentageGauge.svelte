<script lang="ts">
  let {
    label,
    value,
    detail,
    meta,
    warningAt,
    dangerAt
  }: {
    label: string;
    value: number | null | undefined;
    detail?: string;
    meta?: string;
    warningAt?: number;
    dangerAt?: number;
  } = $props();

  let normalized = $derived(
    value == null || !Number.isFinite(value) ? null : Math.min(100, Math.max(0, value))
  );
  let displayValue = $derived(normalized == null ? '—' : `${normalized.toFixed(0)}%`);
  let isWarning = $derived(
    normalized != null
      && warningAt != null
      && normalized >= warningAt
      && (dangerAt == null || normalized < dangerAt)
  );
  let isDanger = $derived(normalized != null && dangerAt != null && normalized >= dangerAt);
</script>

<article
  class="gauge"
  class:warning={isWarning}
  class:danger={isDanger}
  class:unavailable={normalized == null}
  role="meter"
  aria-label={label}
  aria-valuemin="0"
  aria-valuemax="100"
  aria-valuenow={normalized ?? undefined}
  aria-valuetext={normalized == null ? 'Unavailable' : displayValue}
>
  <header>
    <strong>{label}</strong>
    {#if detail}<small>{detail}</small>{/if}
  </header>

  <div class="arc">
    <svg viewBox="0 0 120 66" aria-hidden="true">
      <path class="track" pathLength="100" d="M 12 58 A 48 48 0 0 1 108 58"></path>
      {#if normalized != null && normalized > 0}
        <path
          class="progress"
          pathLength="100"
          stroke-dasharray={`${normalized} 200`}
          d="M 12 58 A 48 48 0 0 1 108 58"
        ></path>
      {/if}
    </svg>
    {#key displayValue}<span class="value">{displayValue}</span>{/key}
  </div>

  {#if meta}<footer>{meta}</footer>{/if}
</article>

<style>
  /* Hallmark · pre-emit critique: P5 H5 E5 S5 R5 V4
   * component: progress arc · genre: modern-minimal · theme: Cobalt
   */
  .gauge {
    display: grid;
    min-width: 0;
    grid-template-rows: auto 4.25rem auto;
    gap: var(--space-2xs);
    color: var(--color-accent);
  }
  .gauge.warning { color: var(--color-warning); }
  .gauge.danger { color: var(--color-danger); }
  .gauge.unavailable { color: var(--color-muted); }
  header { display: flex; min-width: 0; align-items: baseline; justify-content: space-between; gap: var(--space-xs); }
  header strong { color: var(--color-ink); font-family: var(--font-display); font-size: var(--text-xs); font-weight: 650; }
  header small,
  footer { overflow: hidden; color: var(--color-muted); font-family: var(--font-mono); font-size: var(--text-xs); text-overflow: ellipsis; white-space: nowrap; }
  .arc { position: relative; min-width: 5.5rem; }
  svg { display: block; width: 100%; height: 4.25rem; overflow: visible; }
  path { fill: none; vector-effect: non-scaling-stroke; }
  .track { stroke: var(--color-paper-3); stroke-width: 8; }
  .progress {
    stroke: currentColor;
    stroke-linecap: round;
    stroke-width: 8;
    transition: stroke-dasharray var(--dur-long) var(--ease-out);
  }
  .value {
    position: absolute;
    inset: auto 0 0.25rem;
    color: var(--color-ink);
    font-family: var(--font-mono);
    font-size: var(--text-base);
    font-variant-numeric: tabular-nums;
    font-weight: 680;
    text-align: center;
    animation: value-change var(--dur-short) var(--ease-out);
  }
  footer { text-align: center; }
  @keyframes value-change {
    from { opacity: 0.45; transform: translateY(0.2rem); }
  }
  @media (prefers-reduced-motion: reduce) {
    .progress { transition: none; }
    .value { animation: none; }
  }
</style>
