<script lang="ts">
  import { ZButton, ZIcon, ZModal } from '@shival99/z-ui-svelte';
  import { dockTools, layout } from '../layout.svelte';
  import { TOOLS, toolOf } from '../tools';
  import ToolView from './ToolView.svelte';

  const current = $derived(toolOf(layout.activeTool));
</script>

<!-- Detached tool window. Closing it (x, Escape, backdrop) or "Dock back" returns the tool to the right panel. -->
<ZModal
  bind:open={layout.toolsDetached}
  zWidth="min(960px, 92vw)"
  zHideFooter
  zBodyClass="p-0"
  zMaskClosable
  onCancel={dockTools}
>
  {#snippet header()}
    <div class="flex items-center gap-1 pr-8">
      <span class="mr-2 text-sm font-semibold">{current.label}</span>
      <div class="flex items-center gap-0.5" role="tablist" aria-label="Workspace tools">
        {#each TOOLS as tool (tool.value)}
          <ZButton
            zWave={false}
            role="tab"
            zType="ghost"
            zSize="sm"
            class="h-7 w-8 px-0 {layout.activeTool === tool.value ? 'bg-primary/15 text-primary hover:bg-primary/20' : 'hover:bg-primary/10 hover:text-primary'}"
            aria-selected={layout.activeTool === tool.value}
            aria-label={tool.label}
            title={tool.label}
            onclick={() => (layout.activeTool = tool.value)}
          >
            <ZIcon zType={tool.icon} class="size-4" />
          </ZButton>
        {/each}
      </div>
      <ZButton zWave={false} zType="ghost" zSize="sm" class="ml-auto h-7 gap-1.5 text-xs" onclick={dockTools}>
        <ZIcon zType="lucideMinimize2" class="size-3.5" />
        Dock back
      </ZButton>
    </div>
  {/snippet}

  <div class="flex h-[60vh] min-h-72 flex-col">
    <ToolView tool={layout.activeTool} />
  </div>
</ZModal>
