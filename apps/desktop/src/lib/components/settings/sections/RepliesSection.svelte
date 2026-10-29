<script lang="ts">
  import { fade } from 'svelte/transition';
  import { ZButton, ZSelect, ZSwitch } from '@shival99/z-ui-svelte';
  import { DEFAULT_HOUSE_STYLE } from '../../../chat/content/house-style';
  import { replyStyle } from '../../../chat/content/reply-style.svelte';
  import { VOICES } from '../../../app/text/speech';
</script>

            <div class="flex flex-col gap-4" in:fade={{ duration: 200 }}>
              <section class="flex items-start gap-3 rounded-xl border border-border bg-card/40 p-4">
                <div class="min-w-0 flex-1">
                  <h3 class="text-sm font-semibold">Reply conventions</h3>
                  <p class="mt-1 text-[0.75rem] text-muted-foreground">Given to the agent when a new conversation starts: files come back as links named after the file, with no raw paths in the text, and web pictures are saved locally. Old conversations are not changed.</p>
                </div>
                <ZSwitch zVariant="dot" checked={replyStyle.houseStyle} zSize="sm" onChange={(value) => (replyStyle.houseStyle = value)} />
              </section>
              {#if replyStyle.houseStyle}
                <section class="rounded-xl border border-border p-4">
                  <label class="text-sm font-medium" for="house-style-text">Wording</label>
                  <textarea
                    id="house-style-text"
                    rows="6"
                    class="mt-2 w-full resize-y rounded-lg border border-border bg-background p-2 text-sm"
                    placeholder={DEFAULT_HOUSE_STYLE}
                    value={replyStyle.houseStyleText}
                    oninput={(event) => (replyStyle.houseStyleText = event.currentTarget.value)}
                  ></textarea>
                  <div class="mt-2 flex items-center justify-between text-[0.75rem] text-muted-foreground">
                    <span>Blank uses the built-in wording shown above.</span>
                    <ZButton zWave={false} zType="outline" zSize="sm" class="h-7 px-3 text-sm" disabled={!replyStyle.houseStyleText} onclick={() => (replyStyle.houseStyleText = '')}>Reset</ZButton>
                  </div>
                </section>
              {/if}
              <section class="flex items-start gap-3 rounded-xl border border-border bg-card/40 p-4">
                <div class="min-w-0 flex-1">
                  <h3 class="text-sm font-semibold">Links for file paths</h3>
                  <p class="mt-1 text-[0.75rem] text-muted-foreground">A path in an answer (Windows, macOS or Linux, even a raw one) becomes a link showing the file name. Click opens it; right-click opens the folder or copies the path. Paths in code are left alone.</p>
                </div>
                <ZSwitch zVariant="dot" checked={replyStyle.linkPaths} zSize="sm" onChange={(value) => (replyStyle.linkPaths = value)} />
              </section>
              <section class="flex items-start gap-3 rounded-xl border border-border bg-card/40 p-4">
                <div class="min-w-0 flex-1">
                  <h3 class="text-sm font-semibold">Pictures from the web</h3>
                  <p class="mt-1 text-[0.75rem] text-muted-foreground">Shown through the app, from a local copy. Off: the picture is asked for straight from its site (the window may refuse it).</p>
                </div>
                <ZSwitch zVariant="dot" checked={replyStyle.proxyImages} zSize="sm" onChange={(value) => (replyStyle.proxyImages = value)} />
              </section>
              <section class="flex items-start gap-3 rounded-xl border border-border bg-card/40 p-4">
                <div class="min-w-0 flex-1">
                  <h3 class="text-sm font-semibold">Read aloud: Vietnamese voice</h3>
                  <p class="mt-1 text-[0.75rem] text-muted-foreground">The offline voice (Piper) that reads Vietnamese. Each one is downloaded the first time it is used. Answers that are all English use the system voice.</p>
                </div>
                <div class="w-56 shrink-0">
                  <ZSelect zOptions={VOICES} value={replyStyle.voice} onValueChange={(value) => (replyStyle.voice = String(value ?? 'vais1000'))} />
                </div>
              </section>
            </div>
