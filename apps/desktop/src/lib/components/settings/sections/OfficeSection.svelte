<script lang="ts">
  import { fade } from 'svelte/transition';
  import { ZButton, ZIcon } from '@shival99/z-ui-svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import { toast } from '../../../app/platform/toast';
  import { inTauri } from '../../../app/platform/history';
  import { settings } from '../../../agents/connectors.svelte';

  const icon = (name: string) => name as ZIconName;

  // Office files: OfficeCLI (officecli.rs) is downloaded on the first install, then registered with the agents.
  type OfficeStatus = { supported: boolean; installed: boolean; version: string | null };
  let office = $state<OfficeStatus | null>(null);
  let officeBusy = $state(false);
  let officeNote = $state('');
  $effect(() => {
    if (!settings.open || settings.section !== 'office' || !inTauri()) return;
    void import('@tauri-apps/api/core').then(({ invoke }) => invoke<OfficeStatus>('officecli_status')).then((value) => (office = value)).catch(() => (office = null));
  });
  async function installOffice() {
    officeBusy = true;
    officeNote = '';
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      const setup = await invoke<{ version: string; agents: { agent: string; ok: boolean; message: string }[]; skill: string }>('officecli_install');
      const failed = setup.agents.filter((item) => !item.ok).map((item) => item.agent);
      officeNote = `OfficeCLI ${setup.version} installed. Skill: ${setup.skill}.${failed.length ? ` Not registered: ${failed.join(', ')}.` : ''}`;
      office = await invoke<OfficeStatus>('officecli_status');
    } catch (error) {
      toast.error(String(error));
    } finally {
      officeBusy = false;
    }
  }
</script>

            <div class="flex flex-col gap-4" in:fade={{ duration: 200 }}>
              <section class="rounded-xl border border-border p-4">
                <div class="flex items-start gap-3">
                  <div class="min-w-0 flex-1">
                    <h3 class="text-sm font-semibold">OfficeCLI</h3>
                    <p class="mt-1 text-[0.75rem] text-muted-foreground">Lets agents create, read and edit Word (.docx), Excel (.xlsx) and PowerPoint (.pptx) files, with no Office installed. Downloaded once from its GitHub release, then given to Claude, Codex, Antigravity and Copilot as an MCP server and a skill.</p>
                  </div>
                  <ZButton zWave={false} zType="outline" zSize="sm" class="h-8 shrink-0 gap-1.5 px-3 text-sm" disabled={officeBusy || !office?.supported || !inTauri()} onclick={() => void installOffice()}>
                    <ZIcon zType={icon(officeBusy ? 'lucideLoaderCircle' : office?.installed ? 'lucideRefreshCw' : 'lucideDownload')} class="size-3.5 {officeBusy ? 'animate-spin' : ''}" />
                    {officeBusy ? 'Installing…' : office?.installed ? 'Update' : 'Install'}
                  </ZButton>
                </div>
                <p class="mt-3 flex flex-wrap items-center gap-1.5 text-[0.75rem] text-muted-foreground">
                  {#if !inTauri()}Needs the desktop app.
                  {:else if office === null}Checking…
                  {:else if !office.supported}No OfficeCLI release for this system.
                  {:else if office.installed}
                    <span class="inline-flex items-center gap-1 rounded bg-emerald-500/15 px-1.5 py-px text-[0.6875rem] font-medium text-emerald-600 dark:text-emerald-400">
                      <ZIcon zType={icon('lucideCheck')} class="size-3" /> Installed
                    </span>
                    {#if office.version}<span class="rounded bg-muted px-1.5 py-px font-mono text-[0.6875rem] text-foreground/80">{office.version}</span>{/if}
                  {:else}<span class="rounded bg-muted px-1.5 py-px text-[0.6875rem] font-medium">Not installed</span>{/if}
                </p>
                {#if officeNote}<p class="mt-2 text-[0.75rem] text-muted-foreground">{officeNote}</p>{/if}
              </section>
            </div>
