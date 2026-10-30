<script lang="ts">
  import { fade } from 'svelte/transition';
  import { ZButton, ZIcon, ZSwitch } from '@shival99/z-ui-svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import ClaudeCreditCard from '../ClaudeCreditCard.svelte';
  import AgentLogo from '../../ui/AgentLogo.svelte';
  import { openExternal } from '../../../browser/browser.svelte';
  import { brandLogoFor, providerLogo } from '../../../agents/brand-icons';
  import { connectors, removeProvider, scanLocal } from '../../../agents/connectors.svelte';
  import { zModalService } from '@shival99/z-ui-svelte';
  import { FREE_CLOUD, KINDS, isClaudeConsole, samePreset, type ProviderForm } from './provider-form.svelte';

  // Deleting a provider asks first, in the app's danger confirm (the same as deleting a file).
  function confirmRemove(id: string, name: string) {
    zModalService.danger({
      zTitle: `Delete ${name}?`,
      zContent: 'Remove it from AgentDeck? The server itself is not touched.',
      zOkText: 'Delete',
      zConfirmStyle: 'dialog',
      zCancelText: 'Cancel',
      onOk: () => removeProvider(id),
    });
  }

  let { form }: { form: ProviderForm } = $props();

  const icon = (name: string) => name as ZIconName;
</script>

            <div class="flex flex-col gap-5" in:fade={{ duration: 200 }}>
              <section aria-label="Local servers">
                <div class="mb-2 flex items-center gap-2">
                  <h3 class="text-sm font-semibold tracking-wide text-muted-foreground uppercase">On this computer</h3>
                  <span class="ml-auto"></span>
                  <ZButton zWave={false} zType="outline" zSize="sm" class="h-7 gap-1.5 px-2.5 text-sm" disabled={connectors.scanning} onclick={() => void scanLocal()}>
                    <ZIcon zType={icon('lucideRefreshCw')} class="size-3.5 {connectors.scanning ? 'animate-spin' : ''}" /> Scan
                  </ZButton>
                </div>
                {#if connectors.local.length === 0}
                  <p class="rounded-lg border border-dashed border-border p-3 text-[0.75rem] text-muted-foreground">{connectors.scanning ? 'Looking for Ollama, LM Studio and llama.cpp…' : 'No local model server found (looked for Ollama :11434, LM Studio :1234, llama.cpp :8080). Start one and scan again.'}</p>
                {:else}
                  <ul class="grid gap-2 sm:grid-cols-2">
                    {#each connectors.local as server (server.base_url)}
                      <li class="flex items-center gap-3 rounded-lg border border-border p-3">
                        <span class="grid size-8 place-items-center rounded-md bg-emerald-500/15 text-emerald-600 dark:text-emerald-400"><ZIcon zType={icon('lucideCpu')} class="size-4" /></span>
                        <span class="min-w-0 flex-1 text-sm">
                          <span class="block text-sm font-medium">{server.label}</span>
                          <span class="block truncate text-muted-foreground">{server.models.length} models · {server.base_url}</span>
                        </span>
                        <ZButton zWave={false} zType="outline" zSize="sm" class="h-7 px-2.5 text-sm" onclick={() => form.useLocal(server)}>Add</ZButton>
                      </li>
                    {/each}
                  </ul>
                {/if}
              </section>

              <section aria-label="Cloud providers">
                <h3 class="mb-2 text-sm font-semibold tracking-wide text-muted-foreground uppercase">Cloud providers</h3>
                <p class="mb-2 text-[0.75rem] text-muted-foreground">Claude Console API credit, and official APIs with a free tier for small jobs (an email, a summary, a quick search). The key button opens the page where you create a key; then press + , paste it and Save. Free limits change often.</p>
                <ul class="grid gap-2 sm:grid-cols-2">
                  <ClaudeCreditCard {form} />
                  {#each FREE_CLOUD as preset (preset.name)}
                    <li class="flex min-w-0 items-center gap-2.5 rounded-lg border border-border px-3 py-2 text-sm">
                      <img src={brandLogoFor(preset.base)} alt="" class="size-5 shrink-0 rounded-sm object-contain" />
                      <span class="min-w-0 flex-1">
                        <span class="flex min-w-0 items-center gap-1.5 text-sm font-medium"><span class="truncate">{preset.name.replace(/ \(free\)$/, '')}</span><span class="shrink-0 rounded bg-emerald-500/15 px-1.5 py-px text-[10px] font-medium text-emerald-600 dark:text-emerald-400" title="Free tier: gets web search only, never project data">Free</span></span>
                        <span class="block truncate text-[0.6875rem] text-muted-foreground" title={preset.note}>{preset.note}</span>
                      </span>
                      <ZButton zWave={false} zType="ghost" zSize="sm" class="size-7! shrink-0 p-0" aria-label="Get a {preset.name} API key" title="Get a free API key ({preset.keyUrl.replace('https://', '')})" onclick={() => void openExternal(preset.keyUrl)}>
                        <ZIcon zType={icon('lucideKeyRound')} class="size-3.5" />
                      </ZButton>
                      {#if connectors.custom.some((item) => samePreset(preset.base, item.baseUrl))}
                        <span class="grid size-7 shrink-0 place-items-center text-primary" title="Added"><ZIcon zType={icon('lucideCheck')} class="size-3.5" /></span>
                      {:else}
                        <ZButton zWave={false} zType="outline" zSize="sm" class="size-7! shrink-0 p-0" aria-label="Add {preset.name}" title="Add {preset.name}" onclick={() => form.useFreeCloud(preset)}>
                          <ZIcon zType={icon('lucidePlus')} class="size-3.5" />
                        </ZButton>
                      {/if}
                    </li>
                  {/each}
                </ul>
              </section>

              <section aria-label="Custom providers">
                <div class="mb-2 flex items-center gap-2">
                  <h3 class="text-sm font-semibold tracking-wide text-muted-foreground uppercase">Custom providers</h3>
                  <span class="ml-auto"></span>
                  <ZButton zWave={false} zType="outline" zSize="sm" class="h-7 gap-1.5 px-2.5 text-sm" onclick={() => form.edit()}>
                    <ZIcon zType={icon('lucidePlus')} class="size-3.5" /> Add provider
                  </ZButton>
                </div>


                {#if connectors.custom.length === 0}
                  <p class="rounded-lg border border-dashed border-border p-4 text-center text-[0.75rem] text-muted-foreground">No custom provider yet. Add any OpenAI- or Anthropic-compatible endpoint, or a local Ollama.</p>
                {:else}
                  <ul class="flex flex-col gap-2">
                    {#each connectors.custom as provider (provider.id)}
                      {@const activeKey = provider.keys.find((key) => key.id === provider.activeKeyId) ?? provider.keys[0]}
                      <li class="flex items-center gap-3 rounded-lg border border-border p-3">
                        <span class="grid size-8 shrink-0 place-items-center rounded-md bg-muted">{#if isClaudeConsole(provider)}<AgentLogo agent="claude" class="size-4" />{:else if providerLogo(provider)}<img src={providerLogo(provider)} alt="" class="size-5 object-contain" />{:else}<ZIcon zType={icon('lucideServer')} class="size-4" />{/if}</span>
                        <span class="min-w-0 flex-1">
                          <span class="flex items-center gap-1.5 truncate text-sm font-medium">{provider.name}{#if provider.restricted}<span class="shrink-0 rounded bg-amber-500/15 px-1.5 py-px text-[10px] font-medium text-amber-600 dark:text-amber-400" title="May train on what it is sent: gets no project data">Free tier</span>{/if}{#if provider.autoSwitch}<span class="shrink-0 rounded bg-primary/10 px-1.5 py-px text-[10px] font-medium text-primary">Auto switch</span>{/if}</span>
                          <span class="block truncate text-[0.75rem] text-muted-foreground">{KINDS.find((kind) => kind.value === provider.kind)?.label} · {provider.baseUrl}{provider.defaultModel ? ` · ${provider.defaultModel}` : ''}{activeKey?.apiKey ? ` · ${activeKey.label || 'key'} ••••${activeKey.apiKey.slice(-4)}` : ''}{provider.keys.length > 1 ? ` · ${provider.keys.length} keys` : ''}</span>
                        </span>
                        <ZSwitch zVariant="dot" bind:checked={provider.enabled} zSize="sm" />
                        <ZButton zWave={false} zType="outline" zSize="sm" class="size-7! shrink-0 p-0" aria-label="Edit {provider.name}" onclick={() => form.edit(provider)}>
                          <ZIcon zType={icon('lucidePencil')} class="size-3.5" />
                        </ZButton>
                        <ZButton zWave={false} zType="outline" zSize="sm" class="size-7! shrink-0 p-0 text-muted-foreground hover:!border-destructive/60 hover:!bg-destructive/10 hover:!text-destructive" aria-label="Delete {provider.name}" title="Delete" onclick={() => confirmRemove(provider.id, provider.name)}>
                          <ZIcon zType={icon('lucideTrash2')} class="size-3.5" />
                        </ZButton>
                      </li>
                    {/each}
                  </ul>
                  <p class="mt-3 text-[0.6875rem] leading-snug text-muted-foreground">Saved only on this computer: the key never goes into the source or a build. Enabled providers and their models appear in the chat's model picker.</p>
                {/if}
              </section>
            </div>
