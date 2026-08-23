import { defineConfig, presetWind3 } from 'unocss';

export default defineConfig({
  presets: [presetWind3()],
  shortcuts: {
    panel: 'rounded-lg border border-[var(--border)] bg-[var(--card)] shadow-sm',
    eyebrow: 'text-[0.68rem] font-600 uppercase tracking-[0.12em] text-[var(--muted-foreground)]'
  }
});
