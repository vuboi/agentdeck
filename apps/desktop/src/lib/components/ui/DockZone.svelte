<script lang="ts">
  import { ZButton, ZIcon } from '@shival99/z-ui-svelte';
  import { floatFromZone, layout, undockTool, type DockZone as Zone } from '../../app/ui/layout.svelte';
  import { toolOf } from '../../chat/content/tools';
  import ToolView from '../chat/ToolView.svelte';

  let { zone }: { zone: Zone } = $props();

  const tools = $derived(layout.docked[zone]);
  // Tools in the sidebar zone stack; around the conversation they sit side by side (top/bottom) or stacked (left/right).
  const row = $derived(zone === 'top' || zone === 'bottom');
</script>

<!-- A drop target for floating tool windows, and where those tools live once docked here. -->
<div data-dock-zone={zone} class="flex h-full w-full min-h-0 min-w-0 flex-1 overflow-hidden bg-background {row ? 'flex-row' : 'flex-col'}">
  {#if tools.length === 0}
    <div class="flex flex-1 items-center justify-center p-2 text-center text-sm text-muted-foreground">Drop a tool here</div>
  {/if}
  {#each tools as tool (tool)}
    {@const current = toolOf(tool)}
    <div class="flex min-h-0 min-w-0 flex-1 flex-col {row ? 'border-r' : 'border-b'} border-border last:border-0">
      <div class="flex h-10 shrink-0 items-center gap-1.5 border-b border-border px-2">
        <ZIcon zType={current.icon} class="size-3.5 text-muted-foreground" />
        <span class="truncate text-sm font-medium">{current.label}</span>
        <span class="ml-auto"></span>
        <!-- Grip: press and drag to pull the tool out of this zone into a floating window, or dock it elsewhere. -->
        <span
          class="grid size-7 cursor-grab place-items-center rounded-md text-muted-foreground hover:bg-primary/10 hover:text-primary active:cursor-grabbing"
          role="presentation"
          title="Drag to move"
          onpointerdown={(event) => {
            event.preventDefault();
            floatFromZone(tool, event.clientX, event.clientY);
          }}
        >
          <ZIcon zType="lucideGripVertical" class="size-4" />
        </span>
        <ZButton zWave={false} zType="ghost" zSize="sm" class="size-7! p-0 !bg-transparent !text-muted-foreground hover:!bg-primary/10 hover:!text-primary" aria-label="Dock back to the tools panel" title="Dock back" onclick={() => undockTool(tool)}>
          <ZIcon zType="lucideUndo2" class="size-4" />
        </ZButton>
      </div>
      <div class="flex min-h-0 flex-1 flex-col">
        <ToolView tool={tool} />
      </div>
    </div>
  {/each}
</div>
