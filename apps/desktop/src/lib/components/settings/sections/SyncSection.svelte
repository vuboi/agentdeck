<script lang="ts">
  import { fade } from 'svelte/transition';
  import { ZButton, ZIcon, ZSwitch } from '@shival99/z-ui-svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import { exportArchive, pullArchive, pushArchive } from '../../../chat/content/archive';
  import { syncProviders } from '../../../store/sync';
  import { describeExport, exportData, importData, parseExport, pickExport, type ExportFile } from '../../../app/platform/data-export';
  import { settings } from '../../../agents/connectors.svelte';

  const icon = (name: string) => name as ZIconName;

  let archived = $state<{ messages: number; chats: number } | null>(null);
  let syncNote = $state<{ ok: boolean; text: string } | null>(null);
  let syncing = $state(false);

  async function countArchive() {
    const batch = await exportArchive();
    archived = { messages: batch.messages.length, chats: batch.sessions.length };
  }

  /** Pushes and pulls through the in-memory target: shows the whole sync path works before a real service exists. */
  async function tryPath() {
    syncing = true;
    syncNote = null;
    try {
      const sent = await pushArchive('memory');
      const got = await pullArchive('memory');
      syncNote = { ok: true, text: `Sent ${sent} messages, read ${got} back.` };
    } catch (error) {
      syncNote = { ok: false, text: error instanceof Error ? error.message : String(error) };
    } finally {
      syncing = false;
    }
  }

  $effect(() => {
    if (settings.open && settings.section === 'sync') void countArchive();
  });

  // Export / import of the app's data: a stand-in for sync, so a new install gets its settings back in one step.
  const transfer = $state({ withKeys: false, withHistory: false, busy: '' as '' | 'export' | 'import' });
  let transferNote = $state<{ ok: boolean; text: string } | null>(null);
  /** An export picked for import, waiting for the user to confirm. */
  let pending = $state<ExportFile | null>(null);

  async function runExport() {
    transfer.busy = 'export';
    transferNote = null;
    try {
      const done = await exportData({ withKeys: transfer.withKeys, withHistory: transfer.withHistory });
      if (done) transferNote = { ok: true, text: `Exported ${done.summary} to ${done.path}` };
    } catch (error) {
      transferNote = { ok: false, text: error instanceof Error ? error.message : String(error) };
    } finally {
      transfer.busy = '';
    }
  }

  async function pickImport() {
    transferNote = null;
    try {
      const text = await pickExport();
      if (text === null) return;
      const parsed = parseExport(text);
      if (parsed.ok) pending = parsed.file;
      else transferNote = { ok: false, text: parsed.error };
    } catch (error) {
      transferNote = { ok: false, text: error instanceof Error ? error.message : String(error) };
    }
  }

  async function confirmImport() {
    if (!pending) return;
    transfer.busy = 'import';
    try {
      await importData(pending);
      // Every store read its data at start: reload so the imported settings are the ones in use.
      location.reload();
    } catch (error) {
      transferNote = { ok: false, text: error instanceof Error ? error.message : String(error) };
      transfer.busy = '';
      pending = null;
    }
  }
</script>

            <div class="flex flex-col gap-4" in:fade={{ duration: 200 }}>
              <section class="rounded-xl border border-border p-4">
                <p class="text-sm">AgentDeck keeps its own copy of every chat turn (agent, model, time, answer). Sync will copy it to a place you choose, so history follows you between computers.</p>
                <p class="mt-2 text-[0.75rem] text-muted-foreground">
                  {#if archived}<b class="text-foreground tabular-nums">{archived.messages}</b> messages in <b class="text-foreground tabular-nums">{archived.chats}</b> chats saved so far (from chats made since this was added).{:else}Counting…{/if}
                </p>
              </section>

              <section class="rounded-xl border border-border p-4" aria-label="Export and import">
                <div class="text-sm font-medium">Export and import</div>
                <p class="mt-0.5 text-[0.75rem] text-muted-foreground">Until sync is ready: save settings, custom providers, flows, permissions, layout and theme to a file, and import it on another computer or after a reinstall.</p>
                <fieldset class="m-0 mt-3 flex flex-col gap-2 border-0 p-0" aria-label="What the export holds">
                  <div class="flex min-h-9 items-center gap-2 rounded-md border border-border bg-background px-2.5 py-1.5 text-sm">
                    <ZIcon zType={icon('lucideMessagesSquare')} class="size-4 shrink-0 text-muted-foreground" />
                    <span class="min-w-0 flex-1"><span class="font-medium">Include chat history</span> <span class="text-muted-foreground">(bigger file)</span></span>
                    <ZSwitch zVariant="dot" checked={transfer.withHistory} zSize="sm" onChange={(value) => (transfer.withHistory = value)} />
                  </div>
                  <div class="flex min-h-9 items-center gap-2 rounded-md border border-border bg-background px-2.5 py-1.5 text-sm">
                    <ZIcon zType={icon('lucideKeyRound')} class="size-4 shrink-0 text-muted-foreground" />
                    <span class="min-w-0 flex-1"><span class="font-medium">Include provider API keys</span> <span class="text-muted-foreground">(stored as plain text in the file)</span></span>
                    <ZSwitch zVariant="dot" checked={transfer.withKeys} zSize="sm" onChange={(value) => (transfer.withKeys = value)} />
                  </div>
                </fieldset>
                <div class="mt-3 flex flex-wrap items-center gap-2">
                  <ZButton zWave={false} zType="outline" zSize="sm" class="h-7 gap-1.5 px-2.5 text-sm" disabled={transfer.busy !== ''} onclick={() => void runExport()}>
                    <ZIcon zType={icon(transfer.busy === 'export' ? 'lucideLoaderCircle' : 'lucideDownload')} class="size-3.5 {transfer.busy === 'export' ? 'animate-spin' : ''}" /> Export data
                  </ZButton>
                  <ZButton zWave={false} zType="outline" zSize="sm" class="h-7 gap-1.5 px-2.5 text-sm" disabled={transfer.busy !== ''} onclick={() => void pickImport()}>
                    <ZIcon zType={icon('lucideUpload')} class="size-3.5" /> Import data
                  </ZButton>
                </div>
                {#if pending}
                  <div class="mt-3 rounded-lg border border-amber-500/40 bg-amber-500/10 p-3 text-sm" role="alert">
                    <p>Import {describeExport(pending)} from {new Date(pending.exportedAt).toLocaleString()}? Settings with the same name are replaced, the rest stay. AgentDeck reloads afterwards.{pending.withKeys ? '' : ' The file has no API keys: providers keep the keys saved on this computer.'}</p>
                    <div class="mt-2 flex justify-end gap-2">
                      <ZButton zWave={false} zType="ghost" zSize="sm" class="h-7 px-2.5 text-sm" disabled={transfer.busy === 'import'} onclick={() => (pending = null)}>Cancel</ZButton>
                      <ZButton zWave={false} zSize="sm" class="h-7 gap-1.5 px-2.5 text-sm" disabled={transfer.busy === 'import'} onclick={() => void confirmImport()}>
                        {#if transfer.busy === 'import'}<ZIcon zType={icon('lucideLoaderCircle')} class="size-3.5 animate-spin" />{/if}Import and reload
                      </ZButton>
                    </div>
                  </div>
                {/if}
                {#if transferNote}<p class="mt-2 text-sm break-all {transferNote.ok ? 'text-emerald-600 dark:text-emerald-400' : 'text-destructive'}" role="status">{transferNote.text}</p>{/if}
              </section>

              <ul class="flex flex-col gap-3">
                {#each syncProviders as provider (provider.id)}
                  <li class="rounded-xl border border-border bg-card/40 p-4">
                    <div class="flex items-start gap-3">
                      <span class="grid size-9 shrink-0 place-items-center rounded-lg bg-muted"><ZIcon zType={icon(provider.icon)} class="size-4.5" /></span>
                      <div class="min-w-0 flex-1">
                        <div class="flex flex-wrap items-center gap-2">
                          <h3 class="text-sm font-semibold">{provider.label}</h3>
                          <span class="rounded-full px-2 py-0.5 text-[0.6875rem] {provider.status === 'ready' ? 'bg-emerald-500/15 text-emerald-600 dark:text-emerald-400' : 'bg-amber-500/15 text-amber-600 dark:text-amber-400'}">{provider.status === 'ready' ? 'Test only' : 'Planned'}</span>
                        </div>
                        <p class="mt-0.5 text-[0.75rem] text-muted-foreground">{provider.summary}</p>
                      </div>
                    </div>
                    {#if provider.todo.length > 0}
                      <details class="mt-3 text-sm">
                        <summary class="cursor-pointer text-muted-foreground select-none hover:text-foreground">What is left to build ({provider.todo.length})</summary>
                        <ol class="mt-2 list-decimal space-y-1.5 pl-5 text-muted-foreground">
                          {#each provider.todo as step, index (index)}<li>{step}</li>{/each}
                        </ol>
                      </details>
                    {:else}
                      <div class="mt-3 flex flex-wrap items-center gap-2">
                        <ZButton zWave={false} zType="outline" zSize="sm" class="h-7 gap-1.5 px-2.5 text-sm" disabled={syncing} onclick={() => void tryPath()}>
                          <ZIcon zType={icon(syncing ? 'lucideLoaderCircle' : 'lucideZap')} class="size-3.5 {syncing ? 'animate-spin' : ''}" /> Try the sync path
                        </ZButton>
                        {#if syncNote}<span class="text-sm {syncNote.ok ? 'text-emerald-600 dark:text-emerald-400' : 'text-destructive'}" role="status">{syncNote.text}</span>{/if}
                      </div>
                    {/if}
                  </li>
                {/each}
              </ul>
              <p class="text-[0.75rem] text-muted-foreground">Shared to-do for every service (device id, redaction and encryption, keyring for secrets, scheduler) is in <code>src/lib/sync/index.ts</code>.</p>
            </div>
