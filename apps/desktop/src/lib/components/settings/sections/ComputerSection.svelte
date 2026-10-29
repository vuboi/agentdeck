<script lang="ts">
  import { fade } from 'svelte/transition';
  import { ZSwitch } from '@shival99/z-ui-svelte';
  import { inTauri, isPreviewCopy } from '../../../app/platform/history';

  /** The preference is read and applied at start by the modal shell; this only shows and changes it. */
  let { enabled, onChange }: { enabled: boolean; onChange: (value: boolean) => void } = $props();
</script>

            <div class="flex flex-col gap-4" in:fade={{ duration: 200 }}>
              <section class="flex items-start gap-3 rounded-xl border border-border bg-card/40 p-4">
                <div class="min-w-0 flex-1">
                  <h3 class="text-sm font-semibold">Allow agents to use this Windows computer</h3>
                  <p class="mt-1 text-xs text-muted-foreground">Expose shared AgentDeck MCP tools to Claude, Codex and Antigravity for screenshots, clicks and typing.</p>
                </div>
                <ZSwitch zVariant="dot" checked={enabled} zSize="sm" zDisabled={isPreviewCopy() || !inTauri()} onChange={(value) => onChange(value)} />
              </section>
              <p class="rounded-lg border border-primary/20 bg-primary/5 p-3 text-xs text-muted-foreground">Computer Use is on by default. Agents can capture the primary screen, click, type, press keys and scroll without a separate approval each time. Turning this off immediately blocks future calls. Claude, Codex and Antigravity can use the tools through AgentDeck MCP; the <code>ag computer</code> commands use the same bridge.</p>
              {#if enabled && !inTauri()}<p class="text-xs text-destructive">This feature is available in the Windows desktop app.</p>{/if}
            </div>
