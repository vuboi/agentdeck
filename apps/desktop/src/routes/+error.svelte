<script lang="ts">
  import { page } from '$app/state';
  import { goto } from '$app/navigation';
  import { ZButton } from '@shival99/z-ui-svelte';
  import DevEmpty from '$components/browser/DevEmpty.svelte';

  // A page that does not exist (or fails to load): the same empty state as the Preview and the Audit log, centred.
  const notFound = $derived(page.status === 404);
  const title = $derived(notFound ? `${page.status} · Page not found` : `${page.status} · Something went wrong`);
  const hint = $derived(notFound ? 'This address does not lead anywhere in AgentDeck.' : (page.error?.message ?? 'The page could not be loaded.'));
</script>

<svelte:head><title>{page.status} · AgentDeck</title></svelte:head>

<main class="grid h-screen place-items-center bg-background text-foreground">
  <div class="flex flex-col items-center gap-2">
    <DevEmpty icon={notFound ? 'lucideUnplug' : 'lucideTriangleAlert'} {title} {hint} tone={notFound ? 'muted' : 'error'} />
    <ZButton zWave={false} zType="default" zSize="sm" onclick={() => void goto('/')}>Back to AgentDeck</ZButton>
  </div>
</main>
