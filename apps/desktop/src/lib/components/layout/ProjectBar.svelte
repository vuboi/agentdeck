<script lang="ts">
  import { ZButton, ZIcon, ZPopover, zModalService } from '@shival99/z-ui-svelte';
  import { glide, ROW_ON, TRIGGER_ON_PLAIN } from '../../app/ui/popover';
  import PickerPopover, { type PickerItem } from '../ui/PickerPopover.svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import { inTauri } from '../../app/platform/history';
  import { activeSession, isRunning, isUnstarted, moveDraftToProject, NO_PROJECT, workspace } from '../../chat/sessions.svelte';
  import { toast } from '../../app/platform/toast';
  import { addAccount, cancelLogin, connectors, installed, login, refreshAccounts, refreshAgyAccounts, switchAccount, switchAgyAccount, type Account, type AccountProvider, type AgyAccount } from '../../agents/connectors.svelte';
  import AgentLogo from '../ui/AgentLogo.svelte';
  import QuotaBattery from '../settings/QuotaBattery.svelte';
  import { ticker } from '../../app/platform/clock.svelte';
  import { resetCountdown, steadyReset } from '../../chat/recovery/countdown';

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
  const canQuickAdd = $derived(
    accountProvider === 'agy'
      ? installed('agy')
      : accountProvider
        ? Boolean(connectors.accounts[accountProvider]?.manager)
        : false,
  );
  const canOpenAccountMenu = $derived(canQuickSwitch || canQuickAdd);
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
  let switchingAccount = $state(false);
  let loginLaunched = $state(false);
  let confirmError = $state('');
  let confirmAccount = $state<QuickAccount | null>(null);
  let confirmGeneration = 0;
  let loginRequired = $state<Record<string, boolean>>({});
  const entryKey = (entry: QuickAccount) => `${entry.provider}:${entry.account.id}`;
  /** Match the composer's most restrictive limit; Antigravity only exposes quota for its active account. */
  function quotaRemaining(entry: QuickAccount): number | null {
    const model = session?.model ?? '';
    const family = model.startsWith('gemini-') ? 'gemini' : /^(claude-|gpt-oss-)/.test(model) ? '3p' : null;
    const windows = entry.provider === 'agy'
      ? entry.account.active ? connectors.agyQuota?.groups.find((group) => group.id === family)?.windows ?? [] : []
      : entry.account.usage;
    const remaining = windows.filter((window) => Number.isFinite(window.pct)).map((window) =>
      window.resets_at !== null && window.resets_at * 1000 <= ticker.sec ? 100 : Math.max(0, Math.min(100, 100 - window.pct)),
    );
    return remaining.length ? Math.min(...remaining) : null;
  }
  const labelFor = (provider: AccountProvider | 'agy') => (provider === 'claude' ? 'Claude' : provider === 'codex' ? 'Codex' : provider === 'copilot' ? 'GitHub Copilot' : 'Antigravity');
  async function addQuickAccount() {
    if (!accountProvider || !canQuickAdd) return;
    if (accountProvider === 'agy') {
      if (await login('agy', 'Antigravity')) await refreshAgyAccounts();
      return;
    }
    await addAccount(accountProvider, labelFor(accountProvider));
  }
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
  /**
   * When the account can be used again, or (not at a limit) when its next window resets. A limited account is free once
   * every full window has reset, so that is the latest of those. Antigravity tells its windows only for the account in
   * use (the status line agy writes).
   */
  function nextReset(entry: QuickAccount): { at: number; label: string } | null {
    const now = Date.now() / 1000;
    const windows = entry.provider === 'agy' ? (entry.account.active ? (connectors.agyQuota?.groups.flatMap((group) => group.windows) ?? []) : []) : entry.account.usage;
    const live = windows.filter((window): window is typeof window & { resets_at: number } => window.resets_at !== null && window.resets_at > now);
    const full = live.filter((window) => window.pct >= 100);
    const at = (window: (typeof live)[number]) => steadyReset(`${entry.provider}:${entry.account.id}:${window.label}`, window.resets_at);
    if (full.length) {
      const last = full.reduce((a, b) => (at(b) > at(a) ? b : a));
      return { at: at(last), label: last.label };
    }
    if (!live.length) return null;
    const first = live.reduce((a, b) => (at(b) < at(a) ? b : a));
    return { at: at(first), label: `${first.label} ${Math.round(first.pct)}%` };
  }
  type AccountState = 'in-use' | 'switch' | 'limit' | 'sign-in';
  function accountState(entry: QuickAccount): AccountState {
    if (needsLogin(entry)) return 'sign-in';
    if (exhaustedLimits(entry).length) return 'limit';
    return entry.account.active ? 'in-use' : 'switch';
  }
  const ACCOUNT_ORDER: Record<AccountState, number> = { 'in-use': 0, switch: 1, limit: 2, 'sign-in': 3 };
  const sortedAccountRows = $derived([...accountRows].sort((a, b) => ACCOUNT_ORDER[accountState(a)] - ACCOUNT_ORDER[accountState(b)]));
  function openLoginConfirm(entry: QuickAccount, error = '') {
    confirmAccount = entry;
    loginLaunched = false;
    confirmError = error;
    confirmOpen = true;
    showSignIn();
  }
  let signInRef: ReturnType<typeof zModalService.create<boolean>> | null = null;
  function signInText(entry: QuickAccount) {
    const email = entry.account.email;
    const body =
      entry.provider !== 'agy'
        ? `The login for ${email} needs attention. Sign in again and AgentDeck will retry switching to this account. Cancel stops the sign-in.`
        : loginLaunched
          ? `Finish signing in in the Antigravity window. AgentDeck will save the refreshed credentials for this account, then switch back to ${email}.`
          : `The saved login for ${email} needs attention. Open Antigravity sign-in; after it completes, return here to continue.`;
    return confirmError ? `${body}\n\n${confirmError}` : body;
  }
  /** Shows the sign-in dialog for the current state. Cancel is never blocked: the handlers return at once and the sign-in runs in the background. */
  function showSignIn() {
    const entry = confirmAccount;
    if (!entry) return;
    const previous = signInRef;
    signInRef = zModalService.create<boolean>({
      zTitle: `Sign in to ${labelFor(entry.provider)}`,
      zConfirmType: 'show',
      zConfirmStyle: 'dialog',
      zConfirmIcon: 'lucideLogIn',
      zContent: signInText(entry),
      zOkText: entry.provider === 'agy' ? (loginLaunched ? 'Continue and switch' : 'Open sign-in') : 'Sign in and switch',
      zOkIcon: 'lucideLogIn',
      zCancelText: 'Cancel',
      // The sign-in runs in the background while the dialog waits: Cancel, Escape and the backdrop must still close it.
      zCancelWhileLoading: true,
      zWidth: '440px',
      zOkDisabled: () => confirmBusy,
      onOk: () => {
        void confirmAccountAction();
        return false;
      },
      onCancel: () => {
        closeAccountConfirm();
      },
    });
    previous?.close();
  }
  function beginAccountAction(entry: QuickAccount) {
    if (switchingAccount || confirmOpen) return;
    if (entry.account.active && !needsLogin(entry)) {
      accountOpen = false;
      return;
    }
    accountOpen = false;
    if (needsLogin(entry)) {
      openLoginConfirm(entry);
      return;
    }
    switchingAccount = true;
    let loginError = '';
    const ref = zModalService.show({
      zTitle: 'Switch account?',
      zContent: `Switch the active ${labelFor(entry.provider)} account to ${entry.account.email}?`,
      zConfirmStyle: 'dialog',
      zIcon: 'lucideArrowLeftRight',
      zOkIcon: 'lucideArrowLeftRight',
      zOkText: 'Switch account',
      zCancelWhileLoading: true,
      zCancelText: 'Cancel',
      zWidth: '440px',
      onOk: async () => {
        const ok = entry.provider === 'agy' ? await switchAgyAccount(entry.account) : await switchAccount(entry.provider, entry.account);
        if (ok) {
          delete loginRequired[entryKey(entry)];
          return true;
        }
        const error = connectors.errors[switchErrorKey(entry)] ?? 'Could not switch accounts.';
        if (isAuthFailure(error)) {
          loginRequired[entryKey(entry)] = true;
          loginError = error;
          return true;
        }
        toast.error(error);
        return false;
      },
    });
    // Hand authentication failures to the login dialog after the switch confirmation has closed.
    void ref.closed.then((confirmed) => {
      switchingAccount = false;
      if (confirmed && loginError) openLoginConfirm(entry, loginError);
    });
  }
  function closeAccountConfirm() {
    const provider = confirmAccount?.provider;
    const cancelInFlightLogin = confirmBusy && !loginLaunched && provider;
    confirmGeneration += 1;
    confirmOpen = false;
    confirmAccount = null;
    signInRef?.close();
    signInRef = null;
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
      if (generation === confirmGeneration) {
        confirmBusy = false;
        if (confirmOpen) showSignIn();
      }
    }
  }

  // ---- project ----
  let projectTrigger = $state<HTMLElement>();
  let projectOpen = $state(false);
  const projectItems = $derived<PickerItem<string>[]>(
    choices.map((item) => ({ id: item.id, label: item.name, icon: icon(item.id === NO_PROJECT ? 'lucideMonitor' : 'lucideFolder'), selected: item.id === session?.projectId, data: item.id })),
  );
  function pickProject(id: string) {
    if (session) moveDraftToProject(session.id, id);
    projectOpen = false;
    requestAnimationFrame(() => document.querySelector<HTMLElement>('main[aria-label="Conversation"] form [data-composer-editor]')?.focus());
  }

  // ---- git branch ----
  type Branches = { current: string | null; branches: string[] };
  let branches = $state<Branches | null>(null);
  let branchTrigger = $state<HTMLElement>();
  let branchOpen = $state(false);
  let switching = $state(false);
  const branchItems = $derived<PickerItem<string>[]>(
    (branches?.branches ?? []).map((name) => ({ id: name, label: name, icon: icon('lucideGitBranch'), selected: name === branches?.current, data: name })),
  );

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
  <div class="mx-auto mt-1.5 flex min-w-0 max-w-full items-center gap-1 px-1 text-sm text-muted-foreground">
    <!-- Project -->
    {#if canMove}
      <button
        bind:this={projectTrigger}
        type="button"
        class="inline-flex h-6 min-w-0 max-w-48 cursor-pointer items-center gap-1.5 rounded-md px-1.5 outline-none hover:bg-primary/10 hover:text-primary max-[560px]:max-w-[26%] {projectOpen ? TRIGGER_ON_PLAIN : ''}"
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
        class="inline-flex h-6 min-w-0 max-w-48 cursor-pointer items-center gap-1.5 rounded-md px-1.5 outline-none hover:bg-primary/10 hover:text-primary disabled:cursor-not-allowed disabled:opacity-50 max-[560px]:max-w-[26%] {branchOpen ? TRIGGER_ON_PLAIN : ''}"
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

    {#if accountProvider && canOpenAccountMenu}
      <button
        bind:this={accountTrigger}
        type="button"
        class="ml-auto inline-flex h-6 min-w-0 max-w-[44%] cursor-pointer items-center gap-1.5 rounded-md px-1.5 text-muted-foreground outline-none hover:bg-primary/10 hover:text-primary {accountOpen ? TRIGGER_ON_PLAIN : ''}"
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

  <PickerPopover anchor={projectTrigger} bind:open={projectOpen} items={projectItems} local search={choices.length >= SEARCH_FROM} placeholder="Search projects…" label="Project" empty="No project matches" position="top-left" size="sm" width={220} onpick={(item) => pickProject(item.data)} />

  <PickerPopover anchor={branchTrigger} bind:open={branchOpen} items={branchItems} local search={branchItems.length >= SEARCH_FROM} placeholder="Search branches…" label="Git branch" empty="No branch matches" position="top-left" size="sm" width={220} onpick={(item) => void switchTo(item.data)} />

  <ZPopover anchor={accountTrigger} bind:open={accountOpen} zTrigger="manual" zPosition="top-right" zOffset={6} zWidth={280}>
    {#snippet children()}
      <div class="flex w-full flex-col text-sm" role="listbox" aria-label="{labelFor(accountProvider ?? 'agy')} accounts">
        <div class="flex items-center gap-2 border-b border-border px-3 py-2 text-[0.6875rem] font-medium text-muted-foreground">
          <AgentLogo agent={accountProvider === 'agy' ? 'antigravity' : accountProvider ?? 'antigravity'} class="size-3.5 shrink-0" />
          <span class="min-w-0 flex-1">{labelFor(accountProvider ?? 'agy')} accounts</span>
          {#if canQuickAdd}
            <ZButton zWave={false} zType="ghost" zSize="sm" class="size-6! shrink-0 p-0" aria-label="Add {labelFor(accountProvider ?? 'agy')} account" title="Add account" onclick={() => void addQuickAccount()}>
              <ZIcon zType={icon('lucidePlus')} class="size-3.5" />
            </ZButton>
          {/if}
        </div>
        <div class="flex max-h-[max(180px,30vh)] flex-col overflow-x-hidden overflow-y-auto p-1 [&>button+button]:mt-1 [&>button+button]:before:absolute [&>button+button]:before:inset-x-2 [&>button+button]:before:-top-0.5 [&>button+button]:before:h-px [&>button+button]:before:bg-border [&>button+button]:before:content-['']" {@attach glide()}>
          {#each sortedAccountRows as entry (entryKey(entry))}
            {@const active = entry.account.active}
            {@const expired = needsLogin(entry)}
            {@const exhausted = exhaustedLimits(entry)}
            {@const unavailable = expired || exhausted.length > 0}
            {@const reset = expired ? null : nextReset(entry)}
            <button
              type="button"
              role="option"
              aria-selected={active && !expired}
              data-glide-item={exhausted.length > 0 && !expired ? undefined : ''}
              aria-disabled={exhausted.length > 0 && !expired}
              disabled={exhausted.length > 0 && !expired}
              title={exhausted.length ? `Limit reached: ${exhausted.join(', ')}` : undefined}
              class="relative grid w-full min-w-0 grid-cols-[0.875rem_minmax(0,1fr)_2rem] items-center gap-2 rounded-md px-2.5 py-2 text-left outline-none {exhausted.length ? 'cursor-not-allowed text-muted-foreground/55' : 'cursor-pointer'} {active && !unavailable ? 'bg-primary/10' : ''}"
              onclick={() => beginAccountAction(entry)}
            >
              <ZIcon zType={icon(expired ? 'lucideLogIn' : exhausted.length ? 'lucideCircleOff' : active ? 'lucideCheck' : 'lucideArrowLeftRight')} class="size-3.5 shrink-0 {expired ? 'text-warning' : exhausted.length ? 'text-muted-foreground/55' : active ? 'text-primary' : 'text-muted-foreground'}" />
              <span class="min-w-0 flex-1">
                <span class="block truncate font-medium {unavailable ? 'text-muted-foreground/60' : 'text-foreground'}">{entry.account.email}</span>
                {#if exhausted.length}<span class="mt-0.5 block truncate text-[10px]">Limit reached · {exhausted.join(', ')}{#if reset} · <span class="font-medium text-foreground/80 tabular-nums">resets in {resetCountdown(reset.at, ticker.sec)}</span>{/if}</span>
                {:else if reset}<span class="mt-0.5 block truncate text-[10px] text-muted-foreground">{reset.label} · <span class="tabular-nums">resets in {resetCountdown(reset.at, ticker.sec)}</span></span>
                {:else if entry.account.status}<span class="mt-0.5 block truncate text-[10px] text-muted-foreground">{entry.account.status}</span>{/if}
              </span>
              <QuotaBattery remaining={quotaRemaining(entry)} />
            </button>
          {/each}
        </div>
      </div>
    {/snippet}
  </ZPopover>

{/if}
