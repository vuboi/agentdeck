<script lang="ts">
  import { ZIcon, ZShimmerText } from '@shival99/z-ui-svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import type { CommandBlock } from '../../chat/content/blocks';
  import { describeStep, toolOf } from '../../chat/content/blocks';
  import type { Block } from '../../chat/mock';
  import { modelLabel, modelSources } from '../../agents/models.svelte';
  import { WEB_SEARCH } from '../../chat/content/tool-kind';
  import AgentLogo from '../ui/AgentLogo.svelte';

  let { items, agent = 'codex' }: { items: CommandBlock[]; agent?: string } = $props();
  let expanded = $state<Record<string, boolean>>({});
  let logOpen = $state<Record<string, boolean>>({});
  const keyOf = (item: CommandBlock, index: number) => item.id ?? String(index);
  const live = (item: CommandBlock) => item.task?.status === 'started' || item.task?.status === 'progress';
  const status = (item: CommandBlock) => item.task?.status === 'done' ? 'Completed' : item.task?.status === 'failed' ? 'Failed' : 'Working';
  const logs = (item: CommandBlock) => (item.children ?? []).filter((child): child is Extract<Block, { kind: 'command' | 'response' }> => {
    if (child.kind === 'response') return true;
    if (child.kind !== 'command') return false;
    const emptyWebSearch = child.status === 'ok' && WEB_SEARCH.test(toolOf(child.command)) && !child.output.trim() && !child.image && !child.change && !child.task && !child.children?.length;
    return !emptyWebSearch;
  });
  function identity(item: CommandBlock) {
    const label = item.task?.label ?? '';
    for (const source of modelSources()) {
      const model = source.models().find((entry) => label.startsWith(`${entry.value}:`));
      if (model) return { agent: source.provider, model: model.value, name: `${source.label} · ${model.label}`, task: label.slice(model.value.length + 1).trim() };
    }
    const [prefix, ...rest] = label.split(/:\s*/, 2);
    const looksLikeModelId = /^(?:gpt|claude|gemini|o\d|opencode)[\w./:-]*$/i.test(prefix ?? '');
    return {
      agent,
      model: undefined,
      // The label is the task text: the title names the tool, so the task is not shown twice.
      name: looksLikeModelId ? modelLabel(prefix) : toolOf(item.command) || 'Background task',
      task: looksLikeModelId ? rest.join(': ').trim() : label,
    };
  }
</script>

<section class="block-in w-full max-w-full overflow-hidden rounded-lg border border-border bg-background" aria-label="Agent and background activity">
  <header class="flex items-center gap-2 border-b border-border bg-muted/40 px-3 py-1.5 text-sm">
    <ZIcon zType="lucideGitBranch" class="size-3.5 text-muted-foreground" />
    <span class="font-medium">Agent activity</span>
    <span class="ml-auto text-muted-foreground">{items.length} {items.length === 1 ? 'task' : 'tasks'}</span>
  </header>
  {#each items as item, index (keyOf(item, index))}
    {@const key = keyOf(item, index)}
    {@const isOpen = expanded[key] ?? false}
    {@const task = item.task!}
    {@const info = identity(item)}
    {@const children = logs(item)}
    <div class="{index > 0 ? 'border-t border-border' : ''}">
      <button type="button" class="flex min-h-9 w-full cursor-pointer items-center gap-2 px-3 py-2 text-left text-sm outline-none transition-colors hover:bg-muted/40 focus-visible:ring-2 focus-visible:ring-ring/60" aria-expanded={isOpen} onclick={() => (expanded[key] = !isOpen)}>
        <AgentLogo agent={info.agent} model={info.model} class="size-3.5 shrink-0" />
        <span class="min-w-0 max-w-[40%] truncate font-medium" title={info.name}>
          {#if live(item)}<ZShimmerText zBase="var(--primary)" zPeak="var(--foreground)" zText={info.name} />{:else}{info.name}{/if}
        </span>
        <span class="min-w-0 flex-1 truncate text-muted-foreground" title={info.task || task.summary || item.command}>{info.task || task.summary || item.command}</span>
        <span class="inline-flex w-[6.5rem] shrink-0 items-center gap-1.5 {task.status === 'failed' ? 'text-destructive' : task.status === 'done' ? 'text-emerald-500' : 'text-primary'}">
          {#if live(item)}<span class="size-1.5 shrink-0 animate-pulse rounded-full bg-current" aria-hidden="true"></span>{:else}<ZIcon zType={(task.status === 'done' ? 'lucideCircleCheck' : 'lucideCircleX') as ZIconName} class="size-3.5 shrink-0" />{/if}
          <span class="w-[4.5rem] truncate">{status(item)}</span>
        </span>
        <ZIcon zType="lucideChevronRight" class="size-3.5 shrink-0 text-muted-foreground transition-transform {isOpen ? 'rotate-90' : ''}" />
      </button>
      <div class="grid transition-[grid-template-rows] duration-300 ease-out motion-reduce:transition-none {isOpen ? 'grid-rows-[1fr]' : 'grid-rows-[0fr]'}">
        <div class="min-h-0 overflow-hidden" inert={!isOpen}>
          <div class="border-t border-border/70 bg-muted/20">
            <div class="max-h-[min(60vh,32rem)] overflow-y-auto overscroll-contain">
              <div class="flex flex-col gap-2 px-4 py-3 text-xs">
          {#if task.summary}<p class="m-0 whitespace-pre-wrap break-words text-muted-foreground">{task.summary}</p>{/if}
          {#if info.task && info.task !== task.summary}<p class="m-0 whitespace-pre-wrap break-words text-sm font-medium text-foreground">{info.task}</p>{/if}
          {#if task.tools && !children.length}<p class="m-0 text-muted-foreground">Ran {task.tools} tool{task.tools === 1 ? '' : 's'}</p>{/if}
          {#if children.length}
            <div class="divide-y divide-border/70 overflow-hidden rounded-md border border-border bg-background">
              {#each children as child, childIndex (childIndex)}
                {#if child.kind === 'command'}
                  {@const line = describeStep(child)}
                  {@const logKey = `${key}:${childIndex}`}
                  {@const logExpanded = logOpen[logKey] ?? false}
                  <div>
                    <button type="button" class="flex h-9 w-full cursor-pointer items-center gap-2 px-3 text-left outline-none hover:bg-muted/40 focus-visible:ring-2 focus-visible:ring-ring/60 focus-visible:ring-inset" aria-label={`${logExpanded ? 'Collapse' : 'Expand'} log: ${line.verb} ${line.subject}`} aria-expanded={logExpanded} onclick={() => (logOpen[logKey] = !logExpanded)}>
                      <ZIcon zType={line.icon as ZIconName} class="size-3 shrink-0 {child.status === 'fail' ? 'text-destructive' : child.status === 'running' ? 'text-primary' : 'text-muted-foreground/70'}" />
                      {#if child.status === 'running'}
                        <ZShimmerText class="shrink-0 font-medium" zBase="var(--primary)" zPeak="var(--foreground)" zText={line.verb} />
                      {:else}
                        <span class="shrink-0 text-muted-foreground">{line.verb}</span>
                      {/if}
                      <span class="min-w-0 flex-1 truncate font-medium text-foreground">{line.subject}</span>
                      <ZIcon zType="lucideChevronRight" class="size-3.5 shrink-0 text-muted-foreground transition-transform duration-300 {logExpanded ? 'rotate-90' : ''}" />
                    </button>
                    <div class="grid transition-[grid-template-rows] duration-300 ease-out motion-reduce:transition-none {logExpanded ? 'grid-rows-[1fr]' : 'grid-rows-[0fr]'}">
                      <div class="min-h-0 overflow-hidden" inert={!logExpanded}>
                      <div class="border-t border-border/70 bg-muted/20">
                        <div class="max-h-[min(40vh,20rem)] overflow-y-auto overscroll-contain">
                          <div class="flex flex-col gap-1.5 px-3 py-2">
                            <pre class="m-0 whitespace-pre-wrap break-all font-mono text-xs text-muted-foreground">{child.command}</pre>
                            {#if child.output}<pre class="m-0 whitespace-pre-wrap break-words rounded bg-background p-2 font-mono text-xs text-foreground/90">{child.output}</pre>{:else if child.status === 'running'}<p class="m-0 text-xs text-muted-foreground">Waiting for tool output…</p>{/if}
                          </div>
                        </div>
                      </div>
                      </div>
                    </div>
                  </div>
                {:else if child.kind === 'response' && child.text.trim()}
                  <p class="m-0 bg-primary/5 px-3 py-2 whitespace-pre-wrap break-words text-foreground/90">{child.text.trim()}</p>
                {/if}
              {/each}
            </div>
          {:else if item.output.trim()}
            <pre class="m-0 max-h-56 overflow-auto whitespace-pre-wrap break-words rounded bg-background p-2 font-mono text-xs text-muted-foreground">{item.output}</pre>
          {:else if live(item)}
            <p class="m-0 text-muted-foreground/70 italic">The provider has not exposed child logs yet. This task will update here when more activity arrives.</p>
          {:else}
            <p class="m-0 text-muted-foreground/70 italic">No task output was provided by this provider.</p>
          {/if}
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  {/each}
</section>
