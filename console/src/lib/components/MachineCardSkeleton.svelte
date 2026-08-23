<script lang="ts">
  let { active = false, expanded = false }: { active?: boolean; expanded?: boolean } = $props();
</script>

<article class="skeleton" class:active class:expanded aria-hidden="true">
  <div class="head"><i></i><span></span><b></b></div>
  <div class="metrics">
    <i></i><i></i><i></i><span></span>
  </div>
  {#if expanded}<div class="charts"><i></i><i></i></div>{/if}
</article>

<style>
  /* Hallmark · component: machine loading skeleton · genre: modern-minimal · theme: Cobalt */
  .skeleton {
    width: 100%;
    max-width: var(--machine-card-max);
    min-height: var(--machine-card-collapsed-height);
    overflow: hidden;
    border: var(--rule-hairline) solid var(--color-rule);
    border-radius: var(--radius-panel);
    padding: var(--space-md);
    background: var(--color-paper);
  }
  .head { display: grid; grid-template-columns: 2.25rem minmax(0, 1fr) 5rem; align-items: center; gap: var(--space-sm); }
  .head i { width: 2.25rem; height: 2.25rem; border-radius: 50%; }
  .head span { width: min(14rem, 70%); height: 1.25rem; }
  .head b { width: 5rem; height: 1.25rem; }
  .metrics { display: grid; grid-template-columns: repeat(3, minmax(4.75rem, 0.6fr)) minmax(8rem, 1.4fr); gap: var(--space-sm); margin-top: var(--space-lg); }
  .metrics i { height: 5.5rem; border-radius: 50% 50% var(--radius-control) var(--radius-control); }
  .metrics span { height: 5.5rem; }
  .charts { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: var(--space-sm); margin-top: var(--space-lg); padding-top: var(--space-lg); border-top: var(--rule-hairline) solid var(--color-rule); }
  .charts i { height: 16.625rem; }
  .skeleton i,
  .skeleton span,
  .skeleton b {
    display: block;
    border-radius: var(--radius-control);
    background: linear-gradient(100deg, var(--color-paper-2), var(--color-paper-3), var(--color-paper-2));
    background-size: 220% 100%;
  }
  .skeleton.active i,
  .skeleton.active span,
  .skeleton.active b { animation: shimmer 1.4s linear infinite; }
  @keyframes shimmer { to { background-position: -120% 0; } }
  @media (max-width: 36rem) {
    .metrics { grid-template-columns: repeat(2, minmax(0, 1fr)); }
    .metrics span { grid-column: 1 / -1; }
    .charts { grid-template-columns: 1fr; }
  }
  @media (prefers-reduced-motion: reduce) {
    .skeleton.active i,
    .skeleton.active span,
    .skeleton.active b { animation: none; }
  }
</style>
