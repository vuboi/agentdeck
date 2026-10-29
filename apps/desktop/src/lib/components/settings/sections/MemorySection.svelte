<script lang="ts">
  import { fade } from 'svelte/transition';
  import { ZButton, ZSelect, ZSwitch } from '@shival99/z-ui-svelte';
  import { toast } from '../../../app/platform/toast';
  import { replyStyle } from '../../../chat/content/reply-style.svelte';
  import { readProjectMemory, writeProjectMemory, MEMORY_FILE } from '../../../chat/content/project-memory';
  import { workspace } from '../../../chat/sessions.svelte';
  import { settings } from '../../../agents/connectors.svelte';

  /** The project being edited stays chosen while the user visits other sections: kept by the modal shell. */
  let { memoryProjectId = $bindable('') }: { memoryProjectId: string } = $props();

  let memoryText = $state('');
  let memorySaved = $state('');
  let memoryBusy = $state(false);
  const memoryProject = $derived(workspace.projects.find((project) => project.id === memoryProjectId) ?? workspace.projects[0]);
  $effect(() => {
    const path = memoryProject?.path;
    if (!settings.open || settings.section !== 'memory' || !path) return;
    void readProjectMemory(path).then((text) => {
      if (memoryProject?.path !== path) return;
      memoryText = memorySaved = text;
    });
  });
  async function saveMemory() {
    if (!memoryProject) return;
    memoryBusy = true;
    try {
      await writeProjectMemory(memoryProject.path, memoryText);
      memorySaved = memoryText;
      toast.success('Memory saved');
    } catch (error) {
      toast.error(String(error));
    } finally {
      memoryBusy = false;
    }
  }
</script>

            <div class="flex flex-col gap-4" in:fade={{ duration: 200 }}>
              <section class="flex items-start gap-3 rounded-xl border border-border bg-card/40 p-4">
                <div class="min-w-0 flex-1">
                  <h3 class="text-sm font-semibold">Project memory</h3>
                  <p class="mt-1 text-[0.75rem] text-muted-foreground">A short note per project, kept in <code>{MEMORY_FILE}</code> inside it. Every agent that starts a conversation there reads it, and adds what it learns (a convention, a command that works, a pitfall). The same for Claude, Codex and the rest.</p>
                </div>
                <ZSwitch zVariant="dot" checked={replyStyle.projectMemory} zSize="sm" onChange={(value) => (replyStyle.projectMemory = value)} />
              </section>
              <section class="rounded-xl border border-border p-4">
                {#if workspace.projects.length === 0}
                  <p class="text-[0.75rem] text-muted-foreground">Add a project first.</p>
                {:else}
                  <ZSelect zLabel="Project" zOptions={workspace.projects.map((project) => ({ label: project.name, value: project.id }))} zShowSearch={workspace.projects.length > 8} value={memoryProject?.id} onValueChange={(value) => (memoryProjectId = String(value ?? ''))} />
                  <textarea
                    rows="10"
                    class="mt-3 w-full resize-y rounded-lg border border-border bg-background p-2 font-mono text-sm"
                    placeholder="- Uses pnpm, never npm&#10;- UI copy is Vietnamese&#10;- Run `pnpm check` before finishing"
                    value={memoryText}
                    oninput={(event) => (memoryText = event.currentTarget.value)}
                  ></textarea>
                  <div class="mt-2 flex items-center justify-between text-[0.75rem] text-muted-foreground">
                    <span>The first 3000 characters go to the agent; keep it short. No secrets.</span>
                    <ZButton zWave={false} zSize="sm" class="h-7 px-3 text-sm" disabled={memoryBusy || memoryText === memorySaved} onclick={() => void saveMemory()}>Save</ZButton>
                  </div>
                {/if}
              </section>
            </div>
