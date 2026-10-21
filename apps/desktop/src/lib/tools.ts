import type { ZIconName } from '@shival99/z-ui-svelte';

export type Tool = { value: string; label: string; icon: ZIconName };

/** Workspace tools: shown in the right panel, or detached into a modal. */
export const TOOLS: Tool[] = [
  { value: 'Preview', label: 'Preview', icon: 'lucideGlobe' as ZIconName },
  { value: 'Diff', label: 'Diff', icon: 'lucideGitCompare' as ZIconName },
  { value: 'Workflow', label: 'Workflow', icon: 'lucideWorkflow' as ZIconName },
  { value: 'Usage', label: 'Usage', icon: 'lucideActivity' as ZIconName },
  { value: 'Audit', label: 'Audit', icon: 'lucideShieldCheck' as ZIconName },
];

export const toolOf = (value: string) => TOOLS.find((tool) => tool.value === value) ?? TOOLS[0];
