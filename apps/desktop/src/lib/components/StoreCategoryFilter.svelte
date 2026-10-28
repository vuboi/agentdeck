<script lang="ts">
  import { onMount } from 'svelte';
  import { ZSelect } from '@shival99/z-ui-svelte';
  import { loadStoreCategories, storeCategories } from '../store-categories.svelte';

  let { value = $bindable('all'), class: className = 'w-36 max-w-44' }: { value?: string; class?: string } = $props();
  const options = $derived([
    { label: 'All categories', value: 'all' },
    ...storeCategories.items.map((category) => ({ label: category.label, value: category.id })),
  ]);
  onMount(() => { void loadStoreCategories(); });
</script>

<span class="min-w-0 {className}">
  <ZSelect zSize="sm" class="h-8 min-w-0 text-xs" zOptions={options} {value} onValueChange={(next) => (value = String(next ?? 'all'))} zPlaceholder="All categories" zShowSearch zPlaceholderSearch="Find category…" zDropdownWidth={230} />
</span>
