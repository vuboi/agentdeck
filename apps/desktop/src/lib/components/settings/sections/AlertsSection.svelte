<script lang="ts">
  import { fade } from 'svelte/transition';
  import { ZButton, ZIcon, ZSlider, ZSwitch, zSortable } from '@shival99/z-ui-svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import AgentLogo from '../../ui/AgentLogo.svelte';
  import { testSystemNotification } from '../../../app/platform/system-notifications';
  import { toast } from '../../../app/platform/toast';
  import { allowSystemNotifications, connectors, providerName, setKeepAwake, switchAccount, type warnings } from '../../../agents/connectors.svelte';

  /** The limits that are near, worked out by the modal shell (its nav badge uses them too). */
  let { active }: { active: ReturnType<typeof warnings> } = $props();

  const icon = (name: string) => name as ZIconName;

  function moveChain(from: number, to: number) {
    if (from === to) return;
    const list = [...connectors.recovery.chain];
    const [item] = list.splice(from, 1);
    list.splice(to, 0, item);
    connectors.recovery.chain = list;
  }
</script>

            <div class="flex flex-col gap-5" in:fade={{ duration: 200 }}>
              <section class="flex items-start gap-3 rounded-xl border border-border p-4">
                <span class="grid size-9 shrink-0 place-items-center rounded-lg bg-primary/12 text-primary"><ZIcon zType={icon('lucideBell')} class="size-4.5" /></span>
                <div class="min-w-0 flex-1">
                  <div class="text-sm font-medium">Warn me before a limit</div>
                  <p class="mt-0.5 text-[0.75rem] text-muted-foreground">Every 5 minutes AgentDeck reads the usage of the account in use. When a window reaches the level below, you get a message that names the account and the better one to switch to.</p>
                </div>
                <ZSwitch zVariant="dot" bind:checked={connectors.alerts.enabled} zSize="sm" />
              </section>

              <section class="rounded-xl border border-border p-4 {connectors.alerts.enabled ? '' : 'opacity-50'}">
                <div class="flex items-baseline justify-between">
                  <span class="text-sm font-medium">Warn at</span>
                  <span class="text-sm font-semibold tabular-nums">{connectors.alerts.threshold}%</span>
                </div>
                <div class="mt-3"><ZSlider bind:value={connectors.alerts.threshold} zType="dot" zMin={50} zMax={99} zStep={1} zShowTooltip zDisabled={!connectors.alerts.enabled} /></div>
                <div class="mt-1 flex justify-between text-[0.75rem] text-muted-foreground"><span>50% (early)</span><span>99% (last moment)</span></div>
              </section>

              <section class="rounded-xl border border-border p-4">
                <div class="flex items-start gap-3">
                  <div class="min-w-0 flex-1">
                    <div class="text-sm font-medium">System notification</div>
                    <p class="mt-0.5 text-[0.75rem] text-muted-foreground">Show quota warnings and finished session tasks in Windows, macOS, or Linux notifications.</p>
                  </div>
                  <ZSwitch zVariant="dot" checked={connectors.alerts.system} zSize="sm" onChange={(value) => void allowSystemNotifications(value)} />
                </div>
                <!-- Under the text, apart from the switch: a test is a one-off, the switch is the setting. -->
                <ZButton
                  zWave={false}
                  zType="outline"
                  zSize="sm"
                  class="mt-3 h-7! gap-1.5 px-2.5 text-sm"
                  disabled={!connectors.alerts.system}
                  onclick={async () => {
                    const problem = await testSystemNotification();
                    if (problem) toast.error(problem);
                    else toast.success('Test notification sent. If nothing appeared, check Focus Assist / Do Not Disturb.');
                  }}><ZIcon zType={'lucideBell' as ZIconName} class="size-3.5" /> Send a test</ZButton>
              </section>

              <section class="rounded-xl border border-border p-4">
                <div class="flex items-start gap-3">
                  <div class="min-w-0 flex-1">
                    <div class="text-sm font-medium">Prevent system sleep</div>
                    <p class="mt-0.5 text-[0.75rem] text-muted-foreground">Keeps the computer awake while AgentDeck is open, so a long agent task or a reply being waited for is not cut off when the system suspends. The screen may still turn off. Works on Windows, macOS and Linux.</p>
                  </div>
                  <ZSwitch zVariant="dot" checked={connectors.alerts.keepAwake !== false} zSize="sm" onChange={(value) => void setKeepAwake(value)} />
                </div>
              </section>

              <section class="rounded-xl border border-border p-4">
                <div class="flex items-start gap-3">
                  <div class="min-w-0 flex-1">
                    <div class="text-sm font-medium">Keep working when an agent fails</div>
                    <p class="mt-0.5 text-[0.75rem] text-muted-foreground">A passing error is tried again. At a usage limit or a sign-out the next agent below carries on from where it stopped. Accounts are never switched automatically: switch them yourself.</p>
                  </div>
                  <ZSwitch zVariant="dot" checked={connectors.recovery.enabled} zSize="sm" onChange={(value) => (connectors.recovery = { ...connectors.recovery, enabled: value, chosen: true })} />
                </div>
                <div class="mt-3 flex items-start gap-3 rounded-lg border border-border px-3 py-2.5">
                  <div class="min-w-0 flex-1">
                    <div class="text-sm font-medium">Auto-continue when a limit resets</div>
                    <p class="mt-0.5 text-[0.75rem] text-muted-foreground">When a chat stops at a usage limit and no other agent can take over, it carries on by itself once the limit resets (the time is read from the error or the account's usage). A card above the message box shows the time and lets you cancel or try again now.</p>
                  </div>
                  <ZSwitch zVariant="dot" checked={connectors.recovery.autoResume} zSize="sm" onChange={(value) => (connectors.recovery.autoResume = value)} />
                </div>
                <ol
                  class="mt-3 flex flex-col gap-1 {connectors.recovery.enabled ? '' : 'pointer-events-none opacity-50'}"
                  aria-label="Order of agents to carry on with"
                  {@attach zSortable({ onSort: moveChain, handle: '[data-handle]' })}
                >
                  {#each connectors.recovery.chain as agent, index (agent)}
                    <li class="flex items-center gap-2 rounded-lg border border-border bg-card px-2.5 py-1.5 text-sm">
                      <span class="w-4 text-center tabular-nums text-muted-foreground">{index + 1}</span>
                      <AgentLogo {agent} class="size-4 shrink-0" />
                      <span class="flex-1 capitalize">{agent}</span>
                      <span data-handle class="grid size-6 shrink-0 cursor-grab touch-none place-items-center text-muted-foreground hover:text-foreground active:cursor-grabbing" aria-label="Drag to reorder">
                        <ZIcon zType={'lucideGripVertical' as ZIconName} class="size-3.5" />
                      </span>
                    </li>
                  {/each}
                </ol>
              </section>

              <section aria-label="Right now">
                <h3 class="mb-2 text-sm font-semibold tracking-wide text-muted-foreground uppercase">Right now</h3>
                {#if active.length === 0}
                  <p class="rounded-lg border border-dashed border-border p-3 text-[0.75rem] text-muted-foreground">Nothing is near its limit. {connectors.accounts.codex || connectors.accounts.claude ? '' : 'Install an account switcher in Connectors so usage can be read.'}</p>
                {:else}
                  <ul class="flex flex-col gap-2">
                    {#each active as warning (warning.provider + warning.window.label)}
                      <li class="flex items-center gap-3 rounded-lg border border-destructive/40 bg-destructive/5 p-3 text-sm">
                        <ZIcon zType={icon('lucideShieldAlert')} class="size-4 shrink-0 text-destructive" />
                        <span class="min-w-0 flex-1">
                          <b>{providerName(warning.provider)}</b> {warning.window.label} window at {Math.round(warning.window.pct)}% on {warning.account.email}
                          {#if warning.better}<span class="block text-muted-foreground">Better: {warning.better.email}</span>{/if}
                        </span>
                        {#if warning.better}
                          {@const better = warning.better}
                          <ZButton zWave={false} zType="outline" zSize="sm" class="h-7 shrink-0 px-2.5 text-sm" onclick={() => void switchAccount(warning.provider, better)}>Switch</ZButton>
                        {/if}
                      </li>
                    {/each}
                  </ul>
                {/if}
              </section>
            </div>
