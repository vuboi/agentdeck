<script lang="ts">
  import { fade } from 'svelte/transition';
  import { ZButton, ZIcon, ZSwitch } from '@shival99/z-ui-svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import ClaudeCreditCard from '../ClaudeCreditCard.svelte';
  import { openExternal } from '../../../browser/browser.svelte';
  import { brandLogoFor, providerLogo } from '../../../agents/brand-icons';
  import { connectors, scanLocal } from '../../../agents/connectors.svelte';
  import { FREE_CLOUD, GATEWAYS, KINDS, samePreset, type ProviderForm } from './provider-form.svelte';

  let { form }: { form: ProviderForm } = $props();

  const icon = (name: string) => name as ZIconName;
</script>

            <div class="flex flex-col gap-5" in:fade={{ duration: 200 }}>
              <section aria-label="Local servers">
                <div class="mb-2 flex items-center gap-2">
                  <h3 class="text-xs font-semibold tracking-wide text-muted-foreground uppercase">On this computer</h3>
                  <span class="ml-auto"></span>
                  <ZButton zWave={false} zType="outline" zSize="sm" class="h-7 gap-1.5 px-2.5 text-xs" disabled={connectors.scanning} onclick={() => void scanLocal()}>
                    <ZIcon zType={icon('lucideRefreshCw')} class="size-3.5 {connectors.scanning ? 'animate-spin' : ''}" /> Scan
                  </ZButton>
                </div>
                {#if connectors.local.length === 0}
                  <p class="rounded-lg border border-dashed border-border p-3 text-xs text-muted-foreground">{connectors.scanning ? 'Looking for Ollama, LM Studio and llama.cpp…' : 'No local model server found (looked for Ollama :11434, LM Studio :1234, llama.cpp :8080). Start one and scan again.'}</p>
                {:else}
                  <ul class="grid gap-2 sm:grid-cols-2">
                    {#each connectors.local as server (server.base_url)}
                      <li class="flex items-center gap-3 rounded-lg border border-border p-3">
                        <span class="grid size-8 place-items-center rounded-md bg-emerald-500/15 text-emerald-600 dark:text-emerald-400"><ZIcon zType={icon('lucideCpu')} class="size-4" /></span>
                        <span class="min-w-0 flex-1 text-xs">
                          <span class="block text-sm font-medium">{server.label}</span>
                          <span class="block truncate text-muted-foreground">{server.models.length} models · {server.base_url}</span>
                        </span>
                        <ZButton zWave={false} zType="outline" zSize="sm" class="h-7 px-2.5 text-xs" onclick={() => form.useLocal(server)}>Add</ZButton>
                      </li>
                    {/each}
                  </ul>
                {/if}
              </section>

              <section aria-label="Cloud providers">
                <h3 class="mb-2 text-xs font-semibold tracking-wide text-muted-foreground uppercase">Cloud providers</h3>
                <p class="mb-2 text-xs text-muted-foreground">Claude Console API credit, and official APIs with a free tier for small jobs (an email, a summary, a quick search). The key button opens the page where you create a key; then press + , paste it and Save. Free limits change often.</p>
                <ul class="grid gap-2 sm:grid-cols-2">
                  <ClaudeCreditCard />
                  {#each FREE_CLOUD as preset (preset.name)}
                    <li class="flex min-w-0 items-center gap-2.5 rounded-lg border border-border px-3 py-2 text-xs">
                      <img src={brandLogoFor(preset.base)} alt="" class="size-5 shrink-0 rounded-sm object-contain" />
                      <span class="min-w-0 flex-1">
                        <span class="flex min-w-0 items-center gap-1.5 text-sm font-medium"><span class="truncate">{preset.name.replace(/ \(free\)$/, '')}</span><span class="shrink-0 rounded bg-emerald-500/15 px-1.5 py-px text-[10px] font-medium text-emerald-600 dark:text-emerald-400" title="Free tier: gets web search only, never project data">Free</span></span>
                        <span class="block truncate text-muted-foreground" title={preset.note}>{preset.note}</span>
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

              <section aria-label="Self-hosted gateways">
                <h3 class="mb-2 text-xs font-semibold tracking-wide text-muted-foreground uppercase">Self-hosted gateways</h3>
                <ul class="grid gap-2 sm:grid-cols-2">
                  {#each GATEWAYS as preset (preset.name)}
                    <li class="flex min-w-0 items-center gap-2.5 rounded-lg border border-border px-3 py-2 text-xs">
                      <span class="grid size-5 shrink-0 place-items-center text-muted-foreground"><ZIcon zType={icon('lucideServer')} class="size-4" /></span>
                      <span class="min-w-0 flex-1">
                        <span class="block truncate text-sm font-medium">{preset.name}</span>
                        <span class="block truncate text-muted-foreground" title={preset.note}>{preset.note}</span>
                      </span>
                      <ZButton zWave={false} zType="ghost" zSize="sm" class="size-7! shrink-0 p-0" aria-label="Open the {preset.name} dashboard for its key" title="Get a {preset.name} key ({preset.keyUrl.replace('http://', '')})" onclick={() => void openExternal(preset.keyUrl)}>
                        <ZIcon zType={icon('lucideKeyRound')} class="size-3.5" />
                      </ZButton>
                      {#if connectors.custom.some((item) => samePreset(preset.base, item.baseUrl))}
                        <span class="grid size-7 shrink-0 place-items-center text-primary" title="Added"><ZIcon zType={icon('lucideCheck')} class="size-3.5" /></span>
                      {:else}
                        <ZButton zWave={false} zType="outline" zSize="sm" class="size-7! shrink-0 p-0" aria-label="Add {preset.name}" title="Add {preset.name}" onclick={() => form.useGateway(preset)}>
                          <ZIcon zType={icon('lucidePlus')} class="size-3.5" />
                        </ZButton>
                      {/if}
                    </li>
                  {/each}
                </ul>
              </section>

              <section aria-label="Custom providers">
                <div class="mb-2 flex items-center gap-2">
                  <h3 class="text-xs font-semibold tracking-wide text-muted-foreground uppercase">Custom providers</h3>
                  <span class="ml-auto"></span>
                  <ZButton zWave={false} zType="outline" zSize="sm" class="h-7 gap-1.5 px-2.5 text-xs" onclick={() => form.edit()}>
                    <ZIcon zType={icon('lucidePlus')} class="size-3.5" /> Add provider
                  </ZButton>
                </div>


                {#if connectors.custom.length === 0}
                  <p class="rounded-lg border border-dashed border-border p-4 text-center text-xs text-muted-foreground">No custom provider yet. Add any OpenAI- or Anthropic-compatible endpoint, or a local Ollama.</p>
                {:else}
                  <ul class="flex flex-col gap-2">
                    {#each connectors.custom as provider (provider.id)}
                      <li class="flex items-center gap-3 rounded-lg border border-border p-3">
                        <span class="grid size-8 shrink-0 place-items-center rounded-md bg-muted">{#if providerLogo(provider)}<img src={providerLogo(provider)} alt="" class="size-5 object-contain" />{:else}<ZIcon zType={icon('lucideServer')} class="size-4" />{/if}</span>
                        <span class="min-w-0 flex-1">
                          <span class="flex items-center gap-1.5 truncate text-sm font-medium">{provider.name}{#if provider.restricted}<span class="shrink-0 rounded bg-amber-500/15 px-1.5 py-px text-[10px] font-medium text-amber-600 dark:text-amber-400" title="May train on what it is sent: gets no project data">Free tier</span>{/if}</span>
                          <span class="block truncate text-[11px] text-muted-foreground">{KINDS.find((kind) => kind.value === provider.kind)?.label} · {provider.baseUrl}{provider.defaultModel ? ` · ${provider.defaultModel}` : ''}{provider.apiKey ? ` · key ••••${provider.apiKey.slice(-4)}` : ''}</span>
                        </span>
                        <ZSwitch zVariant="dot" bind:checked={provider.enabled} zSize="sm" />
                        <ZButton zWave={false} zType="outline" zSize="sm" class="size-7! shrink-0 p-0" aria-label="Edit {provider.name}" onclick={() => form.edit(provider)}>
                          <ZIcon zType={icon('lucidePencil')} class="size-3.5" />
                        </ZButton>
                        <ZButton zWave={false} zType="outline" zSize="sm" class="size-7! shrink-0 p-0 text-muted-foreground hover:!border-destructive/60 hover:!bg-destructive/10 hover:!text-destructive" aria-label="Delete {provider.name}" title="Delete" onclick={() => (form.confirmDelete = provider.id)}>
                          <ZIcon zType={icon('lucideTrash2')} class="size-3.5" />
                        </ZButton>
                      </li>
                    {/each}
                  </ul>
                  <p class="mt-3 text-[11px] leading-snug text-muted-foreground">Saved only on this computer: the key never goes into the source or a build. Enabled providers and their models appear in the chat's model picker.</p>
                {/if}
              </section>
            </div>
