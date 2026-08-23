<script lang="ts">
  let { distro, size = 40 }: { distro: string; size?: number } = $props();

  const labels: Record<string, string> = {
    ubuntu: 'Ub',
    debian: 'De',
    arch: 'Ar',
    alpine: 'Al',
    openwrt: 'Ow',
    fedora: 'Fe',
    nixos: 'Nx',
    gentoo: 'Ge',
    void: 'Vo'
  };

  let normalized = $derived(distro.trim().toLowerCase());
  let label = $derived(
    Object.entries(labels).find(([name]) => normalized.includes(name))?.[1]
      ?? distro.trim().slice(0, 2)
      ?? 'Li'
  );
</script>

<span
  class="mark"
  style:width={`${size}px`}
  style:height={`${size}px`}
  aria-label={`${distro || 'Linux'} distribution`}
  title={distro || 'Linux'}
>
  {label || 'Li'}
</span>

<style>
  .mark {
    display: inline-grid;
    flex: 0 0 auto;
    place-items: center;
    border: 1px solid var(--border);
    border-radius: calc(var(--radius) - 2px);
    background: var(--muted);
    color: var(--foreground);
    font-size: 0.68rem;
    font-weight: 700;
    letter-spacing: -0.03em;
  }
</style>
