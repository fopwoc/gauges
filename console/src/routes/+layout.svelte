<script lang="ts">
  import '@fontsource-variable/space-grotesk/wght.css';
  import '@fontsource-variable/inter/wght.css';
  import '@fontsource-variable/jetbrains-mono/wght.css';
  import '@unocss/reset/tailwind.css';
  import 'virtual:uno.css';
  import 'uplot/dist/uPlot.min.css';
  import '../app.css';

  let { children } = $props();
</script>

{@render children()}
