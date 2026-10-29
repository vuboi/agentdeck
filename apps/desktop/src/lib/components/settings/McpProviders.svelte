<script lang="ts">
  import { BUILT_IN_AGENTS } from '../../agents/provider-registry';
  import { mcp, setMcpProviders, type McpEntry } from '../../agents/mcp.svelte';
  import ProviderAccess from './ProviderAccess.svelte';

  let { entry }: { entry: McpEntry } = $props();
  const enabled = $derived(entry.builtin ? entry.agents.filter((a) => a !== 'antigravity').concat(mcp.overview?.agy_auto ? ['antigravity'] : []) : entry.enabled_agents ?? entry.agents);
  const options = $derived(BUILT_IN_AGENTS.map((agent) => {
    const supported = ['claude', 'codex', 'copilot', 'antigravity'].includes(agent.id);
    const fixed = entry.builtin && agent.id !== 'antigravity';
    const unsupportedScope = !entry.builtin && agent.id === 'antigravity' && entry.scope.startsWith('project: ');
    return { id: agent.id, label: agent.label, disabled: fixed || !supported || unsupportedScope,
      hint: fixed ? 'Built in' : !supported || unsupportedScope ? 'Not supported' : undefined };
  }));
</script>

<ProviderAccess name={entry.name} selected={enabled} {options} onsave={(next, initial) => setMcpProviders(entry, next, initial)} />
