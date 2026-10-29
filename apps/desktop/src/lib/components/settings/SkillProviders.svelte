<script lang="ts">
  import { BUILT_IN_AGENTS } from '../../agents/provider-registry';
  import { assignSkill, type SkillItem } from '../../store/store.svelte';
  import ProviderAccess from './ProviderAccess.svelte';

  let { skill }: { skill: SkillItem } = $props();
  const options = $derived(BUILT_IN_AGENTS.map((agent) => {
    const own = skill.agents.includes(agent.id) && !skill.linked.includes(agent.id);
    return { id: agent.id, label: agent.label, disabled: own || skill.origin !== 'library', hint: own ? 'Own copy' : undefined };
  }));

  /** Save the latest queued selection, leaving independent provider copies untouched. */
  async function save(next: string[]) {
    const managed = next.filter((id) => !options.find((o) => o.id === id)?.disabled);
    return (await assignSkill(skill.name, managed)) === 'ok';
  }
</script>

<ProviderAccess name={skill.name} selected={skill.agents} {options} onsave={save} />
