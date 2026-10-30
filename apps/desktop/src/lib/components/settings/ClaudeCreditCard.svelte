<script lang="ts">
  import { ZButton, ZIcon, ZModal } from '@shival99/z-ui-svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import AgentLogo from '../ui/AgentLogo.svelte';
  import SecretInput from '../ui/SecretInput.svelte';
  import { openExternal } from '../../browser/browser.svelte';
  import { claudeCreditOn, claudeCreditState, connectors, newProviderId, probeProvider, refreshClaudeCredit, saveProvider, type CustomProvider } from '../../agents/connectors.svelte';
  import { usd } from '../../chat/runs/usage';
  import { ENDPOINTS } from '../../agents/endpoints';
  import { TRIGGER_ON } from '../../app/ui/popover';

  // Settings > Providers: the Claude Console's API credit. Claude Code runs on the API key for the models listed under
  // "Claude credit" in the Claude picker; the Console sign-in (its `sessionKey` cookie) reads what is left of the credit.
  const KEYS_URL = ENDPOINTS.anthropic.keysPage;
  const CONSOLE_URL = ENDPOINTS.anthropic.billingPage;

  let open = $state(false);
  let apiKey = $state('');
  let sessionKey = $state('');
  let testing = $state(false);
  let note = $state<{ ok: boolean; text: string } | null>(null);

  // Added means a Claude Console key is listed under Custom providers, so removing that row brings back the +.
  const added = $derived(connectors.custom.some((item) => item.kind === 'anthropic' && item.baseUrl === ENDPOINTS.anthropic.api));
  const credit = $derived(claudeCreditState.reading);
  const top = $derived(Math.max(claudeCreditState.top, credit?.granted ?? 0, credit?.balance ?? 0));
  const left = $derived(credit && top > 0 ? Math.max(0, Math.min(100, (credit.balance / top) * 100)) : null);
  const expires = $derived(credit?.expires_at ? new Date(credit.expires_at).toLocaleDateString([], { day: 'numeric', month: 'short', year: 'numeric' }) : null);

  // The + always adds another key (each one becomes a row under Custom providers), so the field starts empty.
  function add() {
    apiKey = '';
    sessionKey = connectors.claudeCredit.sessionKey;
    note = null;
    open = true;
  }

  async function save() {
    const key = apiKey.trim();
    if (!key) return;
    testing = true;
    note = null;
    // The key is tried on the API's model list before it is kept: a typo is told here, not on the first message.
    const probe = await probeProvider('anthropic', ENDPOINTS.anthropic.api, key);
    testing = false;
    if (!probe.ok) {
      note = { ok: false, text: probe.error ?? 'The key was refused' };
      return;
    }
    connectors.claudeCredit = { ...connectors.claudeCredit, apiKey: key, sessionKey: sessionKey.trim().replace(/^sessionKey=/, ''), enabled: true };
    // Each key is also a provider of its own, listed under Custom providers (switch, auto switch, edit, delete), so an
    // account can hold several Console keys. A key already listed is not added twice.
    if (!connectors.custom.some((item) => item.keys.some((entry) => entry.apiKey === key))) saveProvider(claudeKeyProvider(key));
    open = false;
    void refreshClaudeCredit();
  }

  function claudeKeyProvider(key: string): CustomProvider {
    const id = newProviderId();
    const keyId = `${id}-key`;
    const count = connectors.custom.filter((item) => item.kind === 'anthropic' && item.baseUrl === ENDPOINTS.anthropic.api).length + 1;
    return {
      id,
      name: 'Claude Console',
      kind: 'anthropic',
      baseUrl: ENDPOINTS.anthropic.api,
      keyEnv: '',
      keys: [{ id: keyId, label: `Key ${count}`, apiKey: key }],
      activeKeyId: keyId,
      autoSwitch: false,
      models: [],
      defaultModel: '',
      enabled: true,
    };
  }

  function remove() {
    connectors.claudeCredit = { apiKey: '', sessionKey: '', enabled: true };
    claudeCreditState.reading = null;
    claudeCreditState.error = null;
    open = false;
  }

  $effect(() => {
    if (connectors.claudeCredit.sessionKey && !claudeCreditState.reading) void refreshClaudeCredit();
  });
</script>

<!-- One row of Settings > Providers > Cloud providers, as the free ones; it is edited in a modal over the settings. -->
<li class="relative flex min-w-0 items-center gap-2.5 overflow-hidden rounded-lg border border-border px-3 py-2 text-sm">
  <span class="grid size-5 shrink-0 place-items-center"><AgentLogo agent="claude" class="size-4" /></span>
  <span class="min-w-0 flex-1">
    <span class="flex min-w-0 items-center gap-1.5 text-sm font-medium"><span class="truncate">Claude Console</span><span class="shrink-0 rounded bg-amber-500/15 px-1.5 py-px text-[10px] font-medium text-amber-600 dark:text-amber-400" title="Paid with API credit: a grant or a top-up">Credit</span></span>
    <span class="block truncate text-[0.6875rem] text-muted-foreground" title={claudeCreditState.error ?? undefined}>
      {#if !connectors.claudeCredit.apiKey}Claude models, prepaid credit
      {:else if credit}{usd(credit.balance)} left{expires ? ` · expires ${expires}` : ''}
      {:else if claudeCreditState.error}<span class="text-destructive">{claudeCreditState.error}</span>
      {:else if connectors.claudeCredit.sessionKey}Reading the credit…
      {:else}Key saved · credit left unknown{/if}
    </span>
  </span>
  {#if connectors.claudeCredit.sessionKey}
    <ZButton zWave={false} zType="ghost" zSize="sm" class="size-7! shrink-0 p-0" aria-label="Read the credit again" title="Read the credit again" disabled={claudeCreditState.busy} onclick={() => void refreshClaudeCredit()}>
      <ZIcon zType={'lucideRefreshCw' as ZIconName} class="size-3.5 {claudeCreditState.busy ? 'animate-spin' : ''}" />
    </ZButton>
  {/if}
  <ZButton zWave={false} zType="ghost" zSize="sm" class="size-7! shrink-0 p-0" aria-label="Create a Claude API key" title="Create an API key ({KEYS_URL.replace('https://', '')})" onclick={() => void openExternal(KEYS_URL)}>
    <ZIcon zType={'lucideKeyRound' as ZIconName} class="size-3.5" />
  </ZButton>
  <!-- A check once a key is saved, as the other providers show; the button still adds another key. -->
  {#if added}
    <ZButton zWave={false} zType="ghost" zSize="sm" class="size-7! shrink-0 p-0 text-primary {open ? TRIGGER_ON : ''}" aria-label="Add another Claude API key" title="Added · add another key" onclick={add}>
      <ZIcon zType={'lucideCheck' as ZIconName} class="size-3.5" />
    </ZButton>
  {:else}
    <ZButton zWave={false} zType="outline" zSize="sm" class="size-7! shrink-0 p-0 {open ? TRIGGER_ON : ''}" aria-label="Add a Claude API key" title="Add an API key" onclick={add}>
      <ZIcon zType={'lucidePlus' as ZIconName} class="size-3.5" />
    </ZButton>
  {/if}
  {#if left !== null && claudeCreditOn()}
    <!-- The credit left, along the row's bottom edge. -->
    <div class="absolute inset-x-0 bottom-0 h-0.5 bg-muted" role="progressbar" aria-label="Claude credit left" aria-valuemin={0} aria-valuemax={100} aria-valuenow={Math.round(left)}>
      <div class="h-full transition-[width] duration-500 {left <= 10 ? 'bg-destructive' : left <= 30 ? 'bg-amber-500' : 'bg-primary'}" style:width="{left}%"></div>
    </div>
  {/if}
</li>

<ZModal bind:open zTitle="Claude Console" zWidth="540px" zBodyClass="min-h-0! scrollbar-full py-3" zOkText="Save" zCancelText="Cancel" zOkDisabled={!apiKey.trim()} zLoading={testing} onOk={() => void save()}>
  <form class="flex flex-col gap-3" onsubmit={(event) => { event.preventDefault(); void save(); }}>
    <p class="m-0 text-[0.75rem] text-muted-foreground">With an API key saved, the Claude picker lists every model again under "Claude credit": Claude Code then runs on that key and the Console credit pays, not your plan.</p>
    <label class="flex flex-col gap-1 text-sm">
      <span class="font-medium">API key</span>
      <SecretInput bind:value={apiKey} kept={connectors.claudeCredit.apiKey} placeholder="sk-ant-api03-…" />
      <span class="text-[0.75rem] text-muted-foreground">Create one in the Console: Settings &gt; API keys (the key button of the row).</span>
    </label>
    <label class="flex flex-col gap-1 text-sm">
      <span class="font-medium">Console sign-in <span class="font-normal text-muted-foreground">· optional, to read the credit left</span></span>
      <SecretInput bind:value={sessionKey} kept={connectors.claudeCredit.sessionKey} placeholder="sk-ant-sid…" />
      <span class="text-[0.6875rem] leading-snug text-muted-foreground">
        The Console has no API for the balance, so AgentDeck reads it the way the Console's billing page does. Open
        <button type="button" class="cursor-pointer text-primary underline underline-offset-2" onclick={() => void openExternal(CONSOLE_URL)}>platform.claude.com</button>
        signed in, press F12, then Application &gt; Cookies &gt; platform.claude.com and copy the value of <code>sessionKey</code>. It works like a password: it stays on this computer and is sent only to platform.claude.com. It lasts until you sign out there.
      </span>
    </label>
    <p class="m-0 flex items-center gap-1.5 text-[0.75rem] text-muted-foreground"><ZIcon zType={'lucideShieldCheck' as ZIconName} class="size-3" /> Both stay on this computer. They are never part of the source, a build, or an export without keys.</p>
    <!-- One line kept for the note, so a result never moves the form. -->
    <p class="m-0 flex h-4 items-center gap-1 text-sm {testing ? 'text-muted-foreground' : 'text-destructive'}" role="status">
      {#if testing}<ZIcon zType={'lucideLoaderCircle' as ZIconName} class="size-3.5 animate-spin" /><span>Checking the key…</span>{:else if note}<ZIcon zType={'lucideCircleX' as ZIconName} class="size-3.5" /><span class="truncate" title={note.text}>{note.text}</span>{/if}
    </p>
    {#if connectors.claudeCredit.apiKey}
      <div><ZButton zWave={false} zType="ghost" zSize="sm" class="h-8 gap-1.5 bg-destructive/10! px-3 text-sm text-destructive! hover:bg-destructive/20!" onclick={remove}><ZIcon zType={'lucideTrash2' as ZIconName} class="size-3.5" /> Remove Claude credit</ZButton></div>
    {/if}
  </form>
</ZModal>
