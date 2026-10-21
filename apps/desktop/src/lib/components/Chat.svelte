<script lang="ts">
  import { messages } from '../mock';
  import Composer from './Composer.svelte';
</script>

<main class="chat" aria-label="Conversation">
  <div class="conversation">
    <div class="conversation-heading"><div><span class="eyebrow">CLAUDE · SONNET 4</span><h1>Build the settings panel</h1></div><button aria-label="More session actions">···</button></div>
    <div class="messages" role="log" aria-live="polite">
      {#each messages as message (message.id)}
        <article class="message {message.role}">
          {#if message.role === 'assistant'}<div class="assistant-mark">✦</div>{:else}<div class="user-mark">JD</div>{/if}
          <div class="message-body"><div class="message-meta">{message.role === 'assistant' ? 'Claude' : 'You'} <span>{message.time}</span></div><p>{message.content}</p></div>
        </article>
      {/each}
    </div>
    <div class="composer-wrap"><Composer /><div class="composer-hint">AgentDeck can make mistakes. Review important changes.</div></div>
  </div>
</main>
