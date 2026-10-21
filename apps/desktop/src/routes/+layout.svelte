<script lang="ts">
  import '../app.css';
  // Importing the store applies the saved theme before the first paint of the app shell.
  import '../lib/theme.svelte';

  let { children } = $props();
</script>

{@render children()}
