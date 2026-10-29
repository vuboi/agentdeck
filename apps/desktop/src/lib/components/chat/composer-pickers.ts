// Rows of the composer's `/` and `@` pickers, built from the app's data.
import type { ZCommandGroup, ZCommandItem, ZIconName } from '@shival99/z-ui-svelte';
import type { PickerItem } from '../ui/PickerPopover.svelte';
import type { SlashItem } from '../../chat/slash.svelte';
import {
  AGENTS,
  MODELS,
  AGENT_ICON as agentIcon,
  workspace,
  type Mention,
} from '../../chat/sessions.svelte';

export const chipIcon: Record<Mention['kind'], ZIconName> = {
  session: 'lucideMessageSquare' as ZIconName,
  project: 'lucideFolder' as ZIconName,
  ai: 'lucideBot' as ZIconName,
};

/** The agent's own commands and skills, plus AgentDeck's, grouped under headings. */
export function buildSlashGroups(
  items: SlashItem[],
  agentLabel: string,
): ZCommandGroup<SlashItem>[] {
  const row = (item: SlashItem): ZCommandItem<SlashItem> => ({
    id: `${item.kind}:${item.origin}:${item.name}`,
    label: item.description
      ? `/${item.name}  ·  ${item.description}`
      : `/${item.name}`,
    icon: (item.kind === 'skill'
      ? 'lucideSparkles'
      : item.kind === 'builtin'
        ? 'lucideZap'
        : 'lucideTerminal') as ZIconName,
    keywords: [item.name, item.description, item.origin],
    data: item,
  });
  const of = (kind: SlashItem['kind']) =>
    items.filter((item) => item.kind === kind).map(row);
  return [
    { heading: 'AgentDeck', items: of('builtin') },
    { heading: `${agentLabel} commands`, items: of('prompt') },
    { heading: `${agentLabel} skills`, items: of('skill') },
  ].filter((group) => group.items.length > 0);
}

/** The picker's rows: the ranked groups flattened, each row under its group's heading. */
export function flatten<T>(
  shown: ZCommandGroup<T>[],
  row: (item: ZCommandItem<T>, group: string) => PickerItem<T>,
): PickerItem<T>[] {
  return shown.flatMap((group) =>
    group.items
      .filter((item) => item.data !== undefined)
      .map((item) => row(item, group.heading ?? '')),
  );
}

/** Sessions, projects and agents that `@` can mention. */
export function buildMentionGroups(): ZCommandGroup<Mention>[] {
  const projectNames = new Map(
    workspace.projects.map((project) => [project.id, project.name]),
  );
  return [
  {
    heading: 'Sessions',
    items: workspace.sessions
      .filter((item) => item.id !== workspace.activeId)
      .map((item) => ({
        id: `session:${item.id}`,
        label: item.title,
        icon: (item.parentId
          ? 'lucideGitFork'
          : 'lucideMessageSquare') as ZIconName,
        keywords: [
          projectNames.get(item.projectId) ?? '',
          item.gitBranch ?? '',
          item.agent,
        ],
        data: {
          kind: 'session' as const,
          id: item.id,
          label: item.title,
        },
      })),
  },
  {
    heading: 'Projects',
    items: workspace.projects.map((item) => ({
      id: `project:${item.id}`,
      label: item.name,
      icon: 'lucideFolder' as ZIconName,
      keywords: [item.path],
      data: {
        kind: 'project' as const,
        id: item.id,
        label: item.name,
      },
    })),
  },
  {
    heading: 'Hand to an agent',
    items: AGENTS.flatMap((entry) =>
      MODELS[entry.value].map((entryModel) => ({
        id: `ai:${entry.value}/${entryModel.value}`,
        label: `${entry.label} · ${entryModel.label}`,
        icon: agentIcon[entry.value],
        data: {
          kind: 'ai' as const,
          id: `${entry.value}/${entryModel.value}`,
          label: `${entry.label} · ${entryModel.label}`,
        },
      })),
    ),
  },
  ];
}
