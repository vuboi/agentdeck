<script lang="ts">
  import { ZLoading } from '@shival99/z-ui-svelte';
  import type { ZLoadingColor, ZLoadingSize, ZLoadingSpinner } from '@shival99/z-ui-svelte';
  import { LOADER } from '../../app/platform/loaders';
  import PhyllotaxisLoader from './PhyllotaxisLoader.svelte';

  let {
    spinner,
    size = '2xs',
    color = 'current',
  }: { spinner: ZLoadingSpinner; size?: ZLoadingSize; color?: ZLoadingColor } = $props();

  const tone = $derived(
    color === 'primary' ? 'text-primary' : color === 'secondary' ? 'text-secondary' : color === 'white' ? 'text-white' : 'text-current',
  );
</script>

{#if spinner === LOADER.background}
  <PhyllotaxisLoader className={tone} />
{:else}
  <ZLoading zLoading zSpinner={spinner} zSize={size} zColor={color} />
{/if}
