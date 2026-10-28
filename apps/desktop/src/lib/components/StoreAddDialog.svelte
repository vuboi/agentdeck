<script lang="ts">
  import { untrack } from 'svelte';
  import { ZButton, ZIcon, ZInput, ZModal, ZTabs } from '@shival99/z-ui-svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import AgentLogo from './AgentLogo.svelte';
  import {
    discardJob,
    installJob,
    pickFolder,
    reviewJob,
    stageSource,
    store,
    type ReviewOutcome,
    type Source,
    type StagedJob,
  } from '../store.svelte';

  const icon = (name: string) => name as ZIconName;

  // Adds something from outside: stage it (nothing runs), scan it, have an agent review it, then install it.
  let { open = $bindable(false), onclose }: { open?: boolean; onclose?: () => void } = $props();

  type Step = 'source' | 'scanned' | 'reviewed';
  let step = $state<Step>('source');
  let kind = $state<'folder' | 'git' | 'zip' | 'url'>('folder');
  let value = $state('');
  let job = $state<StagedJob | null>(null);
  let outcome = $state<ReviewOutcome | null>(null);
  let reviewer = $state<'claude' | 'codex'>('claude');
  let model = $state('');
  let agents = $state<string[]>(['claude']);
  let acknowledged = $state(false);
  let busy = $state(false);

  // Closing the dialog by any route (Cancel, Escape, backdrop) drops the staged job.
  $effect(() => {
    if (!open) untrack(reset);
  });

  const KIND_TABS = [
    { value: 'folder', label: 'Folder' },
    { value: 'git', label: 'Git' },
    { value: 'zip', label: 'Zip' },
    { value: 'url', label: 'URL' },
  ];
  const PLACEHOLDER: Record<string, string> = {
    folder: 'C:\\Users\\you\\skills\\my-skill (contains SKILL.md)',
    git: 'https://github.com/owner/repo',
    zip: 'C:\\Users\\you\\Downloads\\my-skill.zip',
    url: 'https://… a SKILL.md or an MCP config JSON',
  };

  const verdict = $derived(outcome?.verdict ?? job?.scan.verdict ?? 'safe');
  const verdictTone = $derived(verdict === 'block' ? 'text-destructive' : verdict === 'caution' ? 'text-amber-600 dark:text-amber-400' : 'text-emerald-600 dark:text-emerald-400');

  function reset() {
    if (job) void discardJob(job.job_id);
    step = 'source';
    job = null;
    outcome = null;
    value = '';
    acknowledged = false;
    busy = false;
  }

  function close() {
    open = false;
    reset();
    onclose?.();
  }

  function toSource(): Source | null {
    const text = value.trim();
    if (!text) return null;
    if (kind === 'folder') return { kind, path: text };
    if (kind === 'zip') return { kind, path: text };
    return { kind, url: text } as Source;
  }

  async function browse() {
    const picked = await pickFolder();
    if (picked) value = picked;
  }

  async function check() {
    const source = toSource();
    if (!source) return;
    busy = true;
    const staged = await stageSource(source);
    busy = false;
    if (!staged) return;
    job = staged;
    if (staged.kind === 'skill') agents = ['claude'];
    step = 'scanned';
  }

  async function review() {
    if (!job) return;
    busy = true;
    const result = await reviewJob(job.job_id, reviewer, model.trim());
    busy = false;
    if (!result) return;
    outcome = result;
    step = 'reviewed';
  }

  async function install() {
    if (!job || !outcome) return;
    busy = true;
    const done = await installJob(job.job_id, agents, acknowledged);
    busy = false;
    if (done === 'ok') {
      job = null;
      close();
    }
  }

  const canInstall = $derived(
    step === 'reviewed' && !!outcome && outcome.verdict !== 'block' && agents.length > 0 && (outcome.verdict === 'safe' || acknowledged) && job?.kind !== 'plugin' && job?.kind !== 'unknown',
  );
</script>

<ZModal
  bind:open
  zTitle="Import skill or MCP config"
  zWidth="480px"
  zBodyClass="min-h-0! py-3"
  zCancelText="Cancel"
  zOkText={step === 'source' ? 'Check' : step === 'scanned' ? 'Review' : 'Install'}
  zOkDisabled={busy || (step === 'source' && !value.trim()) || (step === 'reviewed' && !canInstall)}
  zLoading={busy}
  onOk={() => (step === 'source' ? check() : step === 'scanned' ? review() : install())}
>
  <div class="flex flex-col gap-3 text-xs">
    {#if step === 'source'}
      <ZTabs zTabs={KIND_TABS} zType="segmented" zSize="sm" zTabsOnly zCache={false} activeTab={kind} onTabChange={(next) => (kind = next as typeof kind)} />
      <ZInput bind:value autofocus zPlaceholder={PLACEHOLDER[kind]} />
      {#if kind === 'folder'}
        <ZButton zWave={false} zType="outline" zSize="sm" class="h-8 w-fit gap-1.5 px-2.5 text-xs" onclick={() => void browse()}>
          <ZIcon zType={icon('lucideFolderOpen')} class="size-3.5" /> Browse
        </ZButton>
      {/if}
      <p class="m-0 leading-snug text-muted-foreground">Choose a skill folder, Git repository, ZIP or MCP config JSON. It is scanned and reviewed before installation. To browse a repository's skills first, add it in Sources and open View skills.</p>
    {:else if step === 'scanned' && job}
      <div class="flex items-center gap-2">
        <span class="font-medium text-foreground capitalize">{job.kind}</span>
        {#if job.skills.length > 0}<span class="text-muted-foreground">{job.skills.join(', ')}</span>{/if}
        <span class="ml-auto font-medium {verdictTone}">Scan: {job.scan.verdict}</span>
      </div>
      <p class="m-0 text-muted-foreground">{job.scan.files} files scanned</p>
      {#if job.scan.findings.length > 0}
        <ul class="m-0 flex max-h-56 list-none flex-col gap-1 overflow-y-auto p-0">
          {#each job.scan.findings.slice(0, 50) as finding, index (index)}
            <li class="rounded-md border border-border p-2 font-mono text-[11px]">
              <span class={finding.severity === 'block' ? 'text-destructive' : 'text-amber-600 dark:text-amber-400'}>[{finding.severity}] {finding.rule}</span>
              <span class="text-muted-foreground">{finding.file}{finding.line ? `:${finding.line}` : ''}</span>
            </li>
          {/each}
        </ul>
      {/if}
      <div class="flex items-center gap-2">
        <AgentLogo agent={reviewer} class="size-3.5" />
        <ZTabs zTabs={[{ value: "claude", label: "Claude" }, { value: "codex", label: "Codex" }]} zType="segmented" zSize="sm" zTabsOnly zCache={false} activeTab={reviewer} onTabChange={(next) => (reviewer = next as typeof reviewer)} />
        <ZInput bind:value={model} zPlaceholder="model (optional)" class="flex-1" />
      </div>
    {:else if step === 'reviewed' && outcome && job}
      <div class="flex items-center gap-2">
        <span class="font-medium {verdictTone}">Verdict: {outcome.verdict}</span>
        <span class="ml-auto text-muted-foreground">scan {outcome.scan}</span>
      </div>
      {#if outcome.review}
        <p class="m-0 leading-snug text-foreground">{outcome.review.summary}</p>
        {#each outcome.review.findings as finding, index (index)}
          <p class="m-0 font-mono text-[11px] text-muted-foreground">{finding.file}{finding.line ? `:${finding.line}` : ''}: {finding.reason}</p>
        {/each}
      {:else}
        <p class="m-0 text-amber-600 dark:text-amber-400">Review failed: {outcome.review_error}. Counted as caution.</p>
      {/if}
      {#if job.kind === 'plugin'}
        <p class="m-0 text-destructive">Plugins from outside cannot be installed yet. Use the agent's plugin marketplace.</p>
      {:else if job.kind === 'unknown'}
        <p class="m-0 text-destructive">No skill, MCP config or plugin found in this source.</p>
      {:else if outcome.verdict === 'block'}
        <p class="m-0 text-destructive">Blocked. Install is off for this source.</p>
      {/if}
      <div class="flex flex-wrap items-center gap-3">
        {#each ['claude', 'codex'] as agent (agent)}
          <label class="flex items-center gap-1.5">
            <input type="checkbox" value={agent} checked={agents.includes(agent)} onchange={(event) => (agents = event.currentTarget.checked ? [...agents, agent] : agents.filter((item) => item !== agent))} />
            <AgentLogo {agent} class="size-3.5" />
            <span class="capitalize">{agent}</span>
          </label>
        {/each}
      </div>
      {#if outcome.verdict === 'caution'}
        <label class="flex items-center gap-2">
          <input type="checkbox" bind:checked={acknowledged} />
          <span>I have read the warnings above</span>
        </label>
      {/if}
    {/if}
  </div>
</ZModal>
