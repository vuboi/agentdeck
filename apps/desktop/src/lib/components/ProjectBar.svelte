<script lang="ts">
  import { ZIcon, ZModal, ZPopover } from '@shival99/z-ui-svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import { inTauri } from '../history';
  import { activeSession, isRunning, isUnstarted, moveDraftToProject, NO_PROJECT, workspace } from '../sessions.svelte';
  import { toast } from '../toast';
  import { cancelLogin, connectors, login, refreshAccounts, refreshAgyAccounts, switchAccount, switchAgyAccount, type Account, type AccountProvider, type AgyAccount } from '../connectors.svelte';
  import AgentLogo from './AgentLogo.svelte';

  // Under the message box: the project the chat works in (switchable while the chat is new, "This computer" by default)
  // and the git branch of that project, which can be switched. A chat that has started keeps its folder.
  const session = $derived(activeSession());
  const project = $derived(workspace.projects.find((item) => item.id === session?.projectId));
  const canMove = $derived(isUnstarted(session));
  const running = $derived(session ? isRunning(session.id) : false);
  const projectName = (id: string) => (id === NO_PROJECT ? 'This computer' : (workspace.projects.find((item) => item.id === id)?.name ?? id));
  // "This computer" first, then the projects the sidebar shows.
  const choices = $derived(
    [...workspace.projects].sort((a, b) => Number(b.id === NO_PROJECT) - Number(a.id === NO_PROJECT)).map((item) => ({ id: item.id, name: projectName(item.id), path: item.path })),
  );

  const SEARCH_FROM = 6;
  const icon = (name: string) => name as ZIconName;

  type QuickAccount = { provider: AccountProvider; account: Account } | { provider: 'agy'; account: AgyAccount };
  const accountProvider = $derived<AccountProvider | 'agy' | null>(
    session?.agent === 'claude' || session?.agent === 'codex' ? session.agent : session?.agent === 'antigravity' ? 'agy' : null,
  );
  const accountRows = $derived.by<QuickAccount[]>(() => {
    if (!accountProvider) return [];
    if (accountProvider === 'agy') return connectors.accounts.agy.map((account) => ({ provider: 'agy', account }));
    const list = connectors.accounts[accountProvider];
    return list?.manager ? list.accounts.map((account) => ({ provider: accountProvider, account })) : [];
  });
  const activeAccount = $derived(accountRows.find((entry) => entry.account.active));
  const canQuickSwitch = $derived(accountRows.length > 1 || Boolean(activeAccount && ('expired' in activeAccount.account && activeAccount.account.expired)));
  let lastAccountProvider: AccountProvider | 'agy' | null | undefined;
  $effect(() => {
    const selected = accountProvider;
    if (selected === lastAccountProvider) return;
    lastAccountProvider = selected;
    if (!selected || !inTauri()) return;
    if (selected === 'agy') void refreshAgyAccounts();
    else void refreshAccounts(selected);
  });

  let accountTrigger = $state<HTMLButtonElement>();
  let accountOpen = $state(false);
  let confirmOpen = $state(false);
  let confirmBusy = $state(false);
  let confirmMode = $state<'switch' | 'login'>('switch');
  let loginLaunched = $state(false);
  let confirmError = $state('');
  let confirmAccount = $state<QuickAccount | null>(null);
  let confirmGeneration = 0;
  let loginRequired = $state<Record<string, boolean>>({});
  const entryKey = (entry: QuickAccount) => `${entry.provider}:${entry.account.id}`;
  const labelFor = (provider: AccountProvider | 'agy') => (provider === 'claude' ? 'Claude' : provider === 'codex' ? 'Codex' : 'Antigravity');
  function needsLogin(entry: QuickAccount) {
    const status = entry.account.status?.toLowerCase() ?? '';
    return Boolean(('expired' in entry.account && entry.account.expired) || loginRequired[entryKey(entry)] || /\b(?:401|403|404)\b|expired|unauthori[sz]ed|authentication required|login required|invalid token|credentials are missing/i.test(status));
  }
  function exhaustedLimits(entry: QuickAccount): string[] {
    if (entry.provider === 'agy') {
      return /quota|limit reached|exhausted/i.test(entry.account.status ?? '') ? ['quota'] : [];
    }
    const now = Date.now();
    return entry.account.usage.filter((window) => window.pct >= 100 && (window.resets_at === null || window.resets_at * 1000 > now)).map((window) => window.label);
  }
  type AccountState = 'in-use' | 'switch' | 'limit' | 'sign-in';
  function accountState(entry: QuickAccount): AccountState {
    if (needsLogin(entry)) return 'sign-in';
    if (exhaustedLimits(entry).length) return 'limit';
    return entry.account.active ? 'in-use' : 'switch';
  }
  const ACCOUNT_ORDER: Record<AccountState, number> = { 'in-use': 0, switch: 1, limit: 2, 'sign-in': 3 };
  const sortedAccountRows = $derived([...accountRows].sort((a, b) => ACCOUNT_ORDER[accountState(a)] - ACCOUNT_ORDER[accountState(b)]));
  function beginAccountAction(entry: QuickAccount) {
    if (entry.account.active && !needsLogin(entry)) {
      accountOpen = false;
      return;
    }
    confirmAccount = entry;
    confirmMode = needsLogin(entry) ? 'login' : 'switch';
    loginLaunched = false;
    confirmError = '';
    accountOpen = false;
    confirmOpen = true;
  }
  function closeAccountConfirm() {
    const provider = confirmAccount?.provider;
    const cancelInFlightLogin = confirmBusy && confirmMode === 'login' && !loginLaunched && provider;
    confirmGeneration += 1;
    confirmOpen = false;
    confirmAccount = null;
    confirmBusy = false;
    loginLaunched = false;
    confirmError = '';
    if (cancelInFlightLogin) void cancelLogin(provider);
  }
  const switchErrorKey = (entry: QuickAccount) => `switch:${entry.provider === 'agy' ? 'agy' : entry.provider}`;
  function isAuthFailure(text: string) {
    return /\b(?:401|403|404)\b|expired|unauthori[sz]ed|authentication required|login required|invalid token|credentials.*missing/i.test(text);
  }
  async function confirmAccountAction() {
    const entry = confirmAccount;
    if (!entry || confirmBusy) return;
    const generation = ++confirmGeneration;
    confirmBusy = true;
    confirmError = '';
    try {
      if (confirmMode === 'switch') {
        const ok = entry.provider === 'agy' ? await switchAgyAccount(entry.account) : await switchAccount(entry.provider, entry.account);
        if (generation !== confirmGeneration) return;
        if (ok) {
          delete loginRequired[entryKey(entry)];
          closeAccountConfirm();
          return;
        }
        const error = connectors.errors[switchErrorKey(entry)] ?? 'Could not switch accounts.';
        confirmError = error;
        if (isAuthFailure(error)) {
          loginRequired[entryKey(entry)] = true;
          confirmMode = 'login';
          loginLaunched = false;
        }
        return;
      }

      if (entry.provider === 'agy') {
        if (!loginLaunched) {
          const ok = await login('agy', 'Antigravity');
          if (generation !== confirmGeneration) return;
          if (ok) {
            loginLaunched = true;
            await refreshAgyAccounts();
            if (generation !== confirmGeneration) return;
            confirmError = connectors.accounts.agy.some((account) => account.email === entry.account.email)
              ? 'Account saved. Choose Continue to switch back to it.'
              : 'Sign-in completed, but the account was not saved. Close and reopen this dialog to retry.';
          } else confirmError = connectors.errors['login:agy'] ?? 'Could not open Antigravity sign-in.';
          return;
        }
        delete loginRequired[entryKey(entry)];
        const refreshed = connectors.accounts.agy.find((account) => account.id === entry.account.id) ?? entry.account;
        const refreshedEntry: QuickAccount = { provider: 'agy', account: refreshed };
        if (needsLogin(refreshedEntry)) {
          confirmError = refreshed.status ?? 'The saved Antigravity login still needs attention.';
          return;
        }
        const switched = await switchAgyAccount(refreshed);
        if (generation !== confirmGeneration) return;
        if (switched) {
          delete loginRequired[entryKey(entry)];
          closeAccountConfirm();
        } else confirmError = connectors.errors['switch:agy'] ?? 'Could not switch accounts.';
        return;
      }

      const loggedIn = await login(entry.provider, labelFor(entry.provider));
      if (generation !== confirmGeneration) return;
      if (!loggedIn) {
        confirmError = connectors.errors[`login:${entry.provider}`] ?? 'Sign-in did not finish.';
        return;
      }
      const refreshed = connectors.accounts[entry.provider]?.accounts.find((account) => account.id === entry.account.id) ?? entry.account;
      const switched = await switchAccount(entry.provider, refreshed);
      if (generation !== confirmGeneration) return;
      if (switched) {
        delete loginRequired[entryKey(entry)];
        closeAccountConfirm();
      } else confirmError = connectors.errors[switchErrorKey(entry)] ?? 'Could not switch accounts.';
    } finally {
      if (generation === confirmGeneration) confirmBusy = false;
    }
  }

  // ---- project ----
  let projectTrigger = $state<HTMLElement>();
  let projectOpen = $state(false);
  let projectQuery = $state('');
  let projectSearch = $state<HTMLInputElement>();
  const shownProjects = $derived(choices.filter((item) => item.name.toLowerCase().includes(projectQuery.trim().toLowerCase())));
  $effect(() => {
    if (projectOpen) requestAnimationFrame(() => projectSearch?.focus());
  });
  function pickProject(id: string) {
    if (session) moveDraftToProject(session.id, id);
    projectOpen = false;
    projectQuery = '';
    requestAnimationFrame(() => document.querySelector<HTMLTextAreaElement>('main[aria-label="Conversation"] form textarea')?.focus());
  }

  // ---- git branch ----
  type Branches = { current: string | null; branches: string[] };
  let branches = $state<Branches | null>(null);
  let branchTrigger = $state<HTMLElement>();
  let branchOpen = $state(false);
  let branchQuery = $state('');
  let branchSearch = $state<HTMLInputElement>();
  let switching = $state(false);
  const shownBranches = $derived((branches?.branches ?? []).filter((name) => name.toLowerCase().includes(branchQuery.trim().toLowerCase())));
  $effect(() => {
    if (branchOpen) requestAnimationFrame(() => branchSearch?.focus());
  });

  async function readBranches(path: string | undefined) {
    if (!path || !inTauri()) {
      branches = null;
      return;
    }
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      const found = await invoke<Branches>('git_branches', { root: path });
      if (project?.path === path) branches = found;
    } catch {
      // Not a repository (or git is missing): there is no branch to show.
      if (project?.path === path) branches = null;
    }
  }
  // Read again whenever the project changes, and when a reply finishes (the agent may have switched branch itself).
  $effect(() => {
    const path = project?.path;
    void running;
    untrackRead(path);
  });
  function untrackRead(path: string | undefined) {
    void readBranches(path);
  }

  async function switchTo(name: string) {
    if (!project?.path || switching || name === branches?.current) {
      branchOpen = false;
      return;
    }
    switching = true;
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      await invoke('git_switch', { root: project.path, branch: name });
      toast.success(`Switched to ${name}`);
      branchOpen = false;
      branchQuery = '';
    } catch (error) {
      // Git's own words: usually local changes that the other branch would overwrite.
      toast.error(String(error).replace(/^error:\s*/i, '').split('\n')[0]);
    } finally {
      switching = false;
      await readBranches(project?.path);
    }
  }
</script>

{#if session}
  <div class="mx-auto mt-1.5 flex min-w-0 max-w-full items-center gap-1 px-1 text-xs text-muted-foreground">
    <!-- Project -->
    {#if canMove}
      <button
        bind:this={projectTrigger}
        type="button"
        class="inline-flex h-6 min-w-0 max-w-48 cursor-pointer items-center gap-1.5 rounded-md px-1.5 outline-none hover:bg-accent hover:text-foreground max-[560px]:max-w-[26%] {projectOpen ? 'bg-accent text-foreground' : ''}"
        aria-label="Project"
        aria-haspopup="listbox"
        aria-expanded={projectOpen}
        onclick={() => (projectOpen = !projectOpen)}
      >
        <ZIcon zType={icon(session.projectId === NO_PROJECT ? 'lucideMonitor' : 'lucideFolder')} class="size-3.5 shrink-0" />
        <span class="truncate">{projectName(session.projectId)}</span>
        <ZIcon zType={icon('lucideChevronDown')} class="size-3 shrink-0" />
      </button>
    {:else}
      <span class="inline-flex h-6 min-w-0 max-w-48 items-center gap-1.5 px-1.5 max-[560px]:max-w-[26%]" title="A chat stays in its folder once it has started">
        <ZIcon zType={icon(session.projectId === NO_PROJECT ? 'lucideMonitor' : 'lucideFolder')} class="size-3.5 shrink-0" />
        <span class="truncate">{projectName(session.projectId)}</span>
      </span>
    {/if}

    <!-- Git branch -->
    {#if branches}
      <button
        bind:this={branchTrigger}
        type="button"
        class="inline-flex h-6 min-w-0 max-w-48 cursor-pointer items-center gap-1.5 rounded-md px-1.5 outline-none hover:bg-accent hover:text-foreground disabled:cursor-not-allowed disabled:opacity-50 max-[560px]:max-w-[26%] {branchOpen ? 'bg-accent text-foreground' : ''}"
        aria-label="Git branch"
        aria-haspopup="listbox"
        aria-expanded={branchOpen}
        title={running ? 'Wait for the reply to finish before switching branch' : 'Switch branch'}
        disabled={running}
        onclick={() => (branchOpen = !branchOpen)}
      >
        <ZIcon zType={icon('lucideGitBranch')} class="size-3.5 shrink-0" />
        <span class="truncate">{branches.current ?? 'detached HEAD'}</span>
        <ZIcon zType={icon(switching ? 'lucideLoaderCircle' : 'lucideChevronDown')} class="size-3 shrink-0 {switching ? 'animate-spin' : ''}" />
      </button>
    {/if}

    {#if accountProvider && canQuickSwitch}
      <button
        bind:this={accountTrigger}
        type="button"
        class="ml-auto inline-flex h-6 min-w-0 max-w-[44%] cursor-pointer items-center gap-1.5 rounded-md px-1.5 text-muted-foreground outline-none hover:bg-accent hover:text-foreground {accountOpen ? 'bg-accent text-foreground' : ''}"
        aria-label="{labelFor(accountProvider)} account: {activeAccount?.account.email ?? 'needs attention'}"
        title="{labelFor(accountProvider)} · {activeAccount?.account.email ?? 'Account needs attention'}"
        aria-haspopup="listbox"
        aria-expanded={accountOpen}
        onclick={() => (accountOpen = !accountOpen)}
      >
        <ZIcon zType={icon(activeAccount && needsLogin(activeAccount) ? 'lucideShieldAlert' : 'lucideArrowLeftRight')} class="size-3.5 shrink-0 {activeAccount && needsLogin(activeAccount) ? 'text-warning' : ''}" />
        <span class="min-w-0 truncate">{activeAccount?.account.email ?? `${labelFor(accountProvider)} account`}</span>
        <ZIcon zType={icon('lucideChevronDown')} class="size-3 shrink-0" />
      </button>
    {/if}
  </div>

  <ZPopover anchor={projectTrigger} bind:open={projectOpen} zTrigger="manual" zPosition="top-left" zOffset={6} zWidth={200}>
    {#snippet children()}
      <div class="flex w-full flex-col text-xs" role="listbox" aria-label="Project">
        {#if choices.length >= SEARCH_FROM}
          <div class="flex h-9 items-center gap-2 border-b border-border px-3">
            <ZIcon zType={icon('lucideSearch')} class="size-3.5 shrink-0 text-muted-foreground" />
            <input class="min-w-0 flex-1 bg-transparent text-xs outline-none placeholder:text-muted-foreground" placeholder="Search projects…" aria-label="Search projects" bind:this={projectSearch} bind:value={projectQuery} />
          </div>
        {/if}
        <div class="flex max-h-44 flex-col gap-0.5 overflow-y-auto p-1">
          {#each shownProjects as item (item.id)}
            <button type="button" role="option" aria-selected={item.id === session.projectId} class="flex w-full cursor-pointer items-center gap-2 rounded-sm px-2 py-1 text-left outline-none hover:bg-accent {item.id === session.projectId ? 'bg-primary/15 text-primary' : ''}" onclick={() => pickProject(item.id)}>
              <ZIcon zType={icon(item.id === NO_PROJECT ? 'lucideMonitor' : 'lucideFolder')} class="size-3.5 shrink-0 text-muted-foreground" />
              <span class="min-w-0 flex-1 truncate">{item.name}</span>
              {#if item.id === session.projectId}<ZIcon zType={icon('lucideCheck')} class="size-3.5 shrink-0" />{/if}
            </button>
          {:else}
            <p class="m-0 px-2 py-2 text-center text-muted-foreground">No project matches</p>
          {/each}
        </div>
      </div>
    {/snippet}
  </ZPopover>

  <ZPopover anchor={branchTrigger} bind:open={branchOpen} zTrigger="manual" zPosition="top-left" zOffset={6} zWidth={180}>
    {#snippet children()}
      <div class="flex w-full flex-col text-xs" role="listbox" aria-label="Git branch">
        {#if (branches?.branches.length ?? 0) >= SEARCH_FROM}
          <div class="flex h-9 items-center gap-2 border-b border-border px-3">
            <ZIcon zType={icon('lucideSearch')} class="size-3.5 shrink-0 text-muted-foreground" />
            <input class="min-w-0 flex-1 bg-transparent text-xs outline-none placeholder:text-muted-foreground" placeholder="Search branches…" aria-label="Search branches" bind:this={branchSearch} bind:value={branchQuery} />
          </div>
        {/if}
        <!-- Fits its branches, and scrolls past a maximum height. -->
        <div class="flex max-h-44 flex-col gap-0.5 overflow-y-auto p-1">
          {#each shownBranches as name (name)}
            <button type="button" role="option" aria-selected={name === branches?.current} class="flex w-full cursor-pointer items-center gap-2 rounded-sm px-2 py-1 text-left outline-none hover:bg-accent {name === branches?.current ? 'bg-primary/15 text-primary' : ''}" disabled={switching} onclick={() => void switchTo(name)}>
              <ZIcon zType={icon('lucideGitBranch')} class="size-3.5 shrink-0 text-muted-foreground" />
              <span class="min-w-0 flex-1 truncate">{name}</span>
              {#if name === branches?.current}<ZIcon zType={icon('lucideCheck')} class="size-3.5 shrink-0" />{/if}
            </button>
          {:else}
            <p class="m-0 px-2 py-2 text-center text-muted-foreground">No branch matches</p>
          {/each}
        </div>
      </div>
    {/snippet}
  </ZPopover>

  <ZPopover anchor={accountTrigger} bind:open={accountOpen} zTrigger="manual" zPosition="top-right" zOffset={6} zWidth={280}>
    {#snippet children()}
      <div class="flex w-full flex-col text-xs" role="listbox" aria-label="{labelFor(accountProvider ?? 'agy')} accounts">
        <div class="flex items-center gap-2 border-b border-border px-3 py-2 text-[11px] font-medium text-muted-foreground">
          <AgentLogo agent={accountProvider === 'agy' ? 'antigravity' : accountProvider ?? 'antigravity'} class="size-3.5 shrink-0" />
          <span>{labelFor(accountProvider ?? 'agy')} accounts</span>
        </div>
        <div class="flex max-h-60 flex-col gap-0.5 overflow-y-auto p-1">
          {#each sortedAccountRows as entry (entryKey(entry))}
            {@const active = entry.account.active}
            {@const expired = needsLogin(entry)}
            {@const exhausted = exhaustedLimits(entry)}
            {@const unavailable = expired || exhausted.length > 0}
            {@const state = accountState(entry)}
            {@const tagStyle = state === 'in-use' ? 'bg-primary/15 text-primary' : 'bg-muted text-muted-foreground'}
            <button
              type="button"
              role="option"
              aria-selected={active && !expired}
              aria-disabled={exhausted.length > 0 && !expired}
              disabled={exhausted.length > 0 && !expired}
              title={exhausted.length ? `Limit reached: ${exhausted.join(', ')}` : undefined}
              class="flex w-full min-w-0 items-center gap-2 rounded-md px-2.5 py-2 text-left outline-none {exhausted.length ? 'cursor-not-allowed text-muted-foreground/55' : 'cursor-pointer hover:bg-accent'} {active && !unavailable ? 'bg-primary/8' : ''}"
              onclick={() => beginAccountAction(entry)}
            >
              <ZIcon zType={icon(expired ? 'lucideLogIn' : exhausted.length ? 'lucideCircleOff' : active ? 'lucideCheck' : 'lucideArrowLeftRight')} class="size-3.5 shrink-0 {expired ? 'text-warning' : exhausted.length ? 'text-muted-foreground/55' : active ? 'text-primary' : 'text-muted-foreground'}" />
              <span class="min-w-0 flex-1">
                <span class="block truncate font-medium {unavailable ? 'text-muted-foreground/60' : 'text-foreground'}">{entry.account.email}</span>
                {#if exhausted.length}<span class="mt-0.5 block truncate text-[10px]">Limit reached · {exhausted.join(', ')}</span>
                {:else if entry.account.status}<span class="mt-0.5 block truncate text-[10px] text-muted-foreground">{entry.account.status}</span>{/if}
              </span>
              <span class="shrink-0 rounded px-1.5 py-0.5 text-[9px] font-medium uppercase {tagStyle}">{state === 'sign-in' ? 'Sign in' : state === 'limit' ? 'Limit' : state === 'in-use' ? 'In use' : 'Switch'}</span>
            </button>
          {/each}
        </div>
      </div>
    {/snippet}
  </ZPopover>

  <ZModal
    bind:open={confirmOpen}
    zTitle={confirmMode === 'login' ? `Sign in to ${labelFor(confirmAccount?.provider ?? 'agy')}` : 'Switch account?'}
    zWidth="440px"
    zBodyClass="min-h-0! py-3"
    zOkText={confirmMode === 'switch' ? 'Switch account' : confirmAccount?.provider === 'agy' ? loginLaunched ? 'Continue and switch' : 'Open sign-in' : 'Sign in and switch'}
    zCancelText="Cancel"
    zLoading={confirmBusy}
    zOkDisabled={!confirmAccount}
    onOk={() => void confirmAccountAction()}
    onCancel={closeAccountConfirm}
  >
    {#if confirmAccount}
      <p class="text-sm text-muted-foreground">
        {#if confirmMode === 'switch'}
          Switch the active {labelFor(confirmAccount.provider)} account to <b class="text-foreground">{confirmAccount.account.email}</b>?
        {:else if confirmAccount.provider === 'agy'}
          {#if loginLaunched}
            Finish signing in in the Antigravity window. AgentDeck will save the refreshed credentials for this account, then switch back to <b class="text-foreground">{confirmAccount.account.email}</b>.
          {:else}
            The saved login for <b class="text-foreground">{confirmAccount.account.email}</b> needs attention. Open Antigravity sign-in; after it completes, return here to continue.
          {/if}
        {:else}
          The login for <b class="text-foreground">{confirmAccount.account.email}</b> needs attention. Sign in again and AgentDeck will retry switching to this account.
        {/if}
      </p>
      {#if confirmError}<p class="mt-3 rounded-md bg-destructive/10 p-2 text-xs text-destructive" role="alert">{confirmError}</p>{/if}
    {/if}
  </ZModal>
{/if}
