import type { ZIconName } from '@shival99/z-ui-svelte';

export type Tool = { value: string; label: string; icon: ZIconName };

/** Workspace tools: shown in the right panel, or detached into a modal. */
export const TOOLS: Tool[] = [
  { value: 'Explore', label: 'Explore', icon: 'lucideFolder' as ZIconName },
  { value: 'Preview', label: 'Preview', icon: 'lucideGlobe' as ZIconName },
  { value: 'Images', label: 'Images', icon: 'lucideImage' as ZIconName },
  { value: 'Diff', label: 'Changes', icon: 'lucideGitCompare' as ZIconName },
  { value: 'Tasks', label: 'Tasks', icon: 'lucideListChecks' as ZIconName },
  { value: 'MCP', label: 'MCP', icon: 'lucidePlug' as ZIconName },
  { value: 'Workflow', label: 'Workflow', icon: 'lucideWorkflow' as ZIconName },
  { value: 'Usage', label: 'Usage', icon: 'lucideActivity' as ZIconName },
];

/** Tools that have a window but no tab in the panel. */
const EXTRA: Tool[] = [{ value: 'DevTools', label: 'Developer tools', icon: 'lucideSquareTerminal' as ZIconName }];

export const toolOf = (value: string) => [...TOOLS, ...EXTRA].find((tool) => tool.value === value) ?? TOOLS[0];
