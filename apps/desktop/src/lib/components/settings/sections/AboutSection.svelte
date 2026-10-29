<script lang="ts">
  import { fade } from 'svelte/transition';
  import { ZButton, ZIcon } from '@shival99/z-ui-svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import Logo from '../../ui/Logo.svelte';
  import { appInfo, runUpdateCheck, updateUi } from '../../../app/platform/updater.svelte';
  import { ENDPOINTS } from '../../../agents/endpoints';
  import { openExternal } from '../../../browser/browser.svelte';

  const icon = (name: string) => name as ZIconName;
</script>

            <div class="flex flex-col items-center gap-5 py-2 text-center" in:fade={{ duration: 200 }}>
              <!-- The logo in a dashed frame, centred: what AgentDeck is, which version, and where to find it. -->
              <div class="grid size-24 place-items-center rounded-3xl border-2 border-dashed border-border bg-card/40">
                <Logo class="size-12" />
              </div>
              <div class="flex flex-col items-center gap-1.5">
                <h2 class="m-0 text-xl font-semibold">AgentDeck</h2>
                <span class="rounded-full border border-border bg-muted/50 px-2.5 py-0.5 font-mono text-xs text-muted-foreground tabular-nums">{appInfo.version ? `v${appInfo.version}` : 'dev build'}</span>
              </div>
              <p class="m-0 max-w-md text-sm leading-relaxed text-muted-foreground">
                One desktop for your AI coding agents: Claude, Codex, Antigravity, OpenCode and any provider, working side by side with a browser, your files and a terminal. Chats, keys and settings stay on this computer.
              </p>
              <div class="flex flex-wrap items-center justify-center gap-2">
                <ZButton zWave={false} zType="outline" zSize="sm" class="h-8 gap-1.5 px-3 text-xs" onclick={() => void openExternal(ENDPOINTS.app.repo)}>
                  <!-- lucide dropped brand icons: the GitHub mark is drawn here so it also shows offline. -->
                  <svg viewBox="0 0 16 16" class="size-3.5" fill="currentColor" aria-hidden="true"><path d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27s1.36.09 2 .27c1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.01 8.01 0 0 0 16 8c0-4.42-3.58-8-8-8Z"/></svg> GitHub
                </ZButton>
                <ZButton zWave={false} zType="outline" zSize="sm" class="h-8 gap-1.5 px-3 text-xs" onclick={() => void openExternal(ENDPOINTS.app.releases)}>
                  <ZIcon zType={icon('lucideTag')} class="size-3.5" /> Releases
                </ZButton>
                {#if appInfo.version}
                  <ZButton zWave={false} zType="outline" zSize="sm" class="h-8 gap-1.5 px-3 text-xs" onclick={() => void openExternal(`${ENDPOINTS.app.releases}/tag/v${appInfo.version}`)}>
                    <ZIcon zType={icon('lucideScrollText')} class="size-3.5" /> What's new in v{appInfo.version}
                  </ZButton>
                {/if}
                <ZButton zWave={false} zType="outline" zSize="sm" class="h-8 gap-1.5 px-3 text-xs" onclick={() => void openExternal(ENDPOINTS.app.issues)}>
                  <ZIcon zType={icon('lucideBug')} class="size-3.5" /> Report an issue
                </ZButton>
              </div>
              <ZButton zWave={false} zSize="sm" class="h-8 gap-1.5 px-4 text-xs" disabled={updateUi.spinning} onclick={() => void runUpdateCheck()}>
                <ZIcon zType={icon(updateUi.spinning ? 'lucideLoaderCircle' : 'lucideRefreshCw')} class="size-3.5 {updateUi.spinning ? 'animate-spin' : ''}" /> {updateUi.spinning ? 'Checking…' : 'Check for updates'}
              </ZButton>
              <p class="m-0 text-[11px] text-muted-foreground">Built with Tauri 2, Svelte 5 and Rust.</p>
            </div>
