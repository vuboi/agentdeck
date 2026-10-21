<script lang="ts">
  let draft = $state('');
  let area: HTMLTextAreaElement;

  function resize() {
    if (!area) return;
    area.style.height = 'auto';
    area.style.height = `${Math.min(area.scrollHeight, 180)}px`;
  }
  function submit() { draft = ''; resize(); }
</script>

<form class="composer" onsubmit={(event) => { event.preventDefault(); submit(); }}>
  <textarea bind:this={area} bind:value={draft} oninput={resize} onkeydown={(event) => { if (event.key === 'Enter' && !event.shiftKey) { event.preventDefault(); submit(); } }} rows="1" aria-label="Message" placeholder="Message AgentDeck…"></textarea>
  <div class="composer-bottom"><div class="composer-tools"><button type="button" aria-label="Attach file">＋</button><button type="button" class="agent-select">✦ Claude <span>⌄</span></button><button type="button" class="model-select">Sonnet 4 <span>⌄</span></button></div><button class="send" type="submit" disabled={!draft.trim()} aria-label="Send message">↑</button></div>
</form>
