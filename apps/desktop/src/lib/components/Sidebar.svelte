<script lang="ts">
  import { sessions } from '../mock';

  let selected = $state('s1');
  const shortcutHint = typeof navigator !== 'undefined' && /Mac|iPhone|iPad/.test(navigator.platform) ? '⌘N' : 'Ctrl+N';

  function agentLabel(agent: string) { return agent[0].toUpperCase() + agent.slice(1); }
</script>

<aside class="sidebar" aria-label="Sessions">
  <button class="new-session" onclick={() => (selected = '')}><span>＋</span> New session <kbd>{shortcutHint}</kbd></button>
  <div class="section-label">RECENT</div>
  <ul class="session-list">
    {#each sessions as session (session.id)}
      <li>
        <button class:active={selected === session.id} class="session" onclick={() => (selected = session.id)} aria-current={selected === session.id ? 'page' : undefined}>
          <span class="status-dot {session.status}" aria-label={session.status}></span>
          <span class="session-copy"><span class="session-title">{session.title}</span><span class="agent-name">{agentLabel(session.agent)}</span></span>
        </button>
      </li>
    {/each}
  </ul>
  <div class="sidebar-footer"><span class="avatar">JD</span><span>Jane Developer</span><span class="more">···</span></div>
</aside>
