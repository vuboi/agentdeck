// Rows of the composer's `/` and `@` pickers, built from the app's data.
import type { ZCommandGroup, ZCommandItem, ZIconName } from '@shival99/z-ui-svelte';
import type { PickerItem } from '../ui/PickerPopover.svelte';
import type { SlashItem } from '../../chat/slash.svelte';
import { allPresets } from '../../tasks/workflows.svelte';
import {
  AGENTS,
  MODELS,
  AGENT_ICON as agentIcon,
  workspace,
  type Mention,
} from '../../chat/sessions.svelte';
import { CREDIT_PREFIX } from '../../agents/credit-model';
import { isFreeModel, isPaidOnlyModel } from '../../agents/models.svelte';

export const chipIcon: Record<Mention['kind'], ZIconName> = {
  session: 'lucideMessageSquare' as ZIconName,
  project: 'lucideFolder' as ZIconName,
  ai: 'lucideBot' as ZIconName,
  workflow: 'lucideWorkflow' as ZIconName,
};

/** Keeps duplicate model names selectable in mentions: plan, credit and paid-only variants must be visible. */
function mentionModelLabel(agent: string, model: { value: string; label: string }) {
  const variant = model.value.startsWith(CREDIT_PREFIX)
    ? 'Credit'
    : isPaidOnlyModel(model)
      ? 'Paid'
      : isFreeModel(model) && !/\bfree\b/i.test(model.label)
        ? 'Free'
        : '';
  return `${agent} · ${model.label}${variant ? ` (${variant})` : ''}`;
}

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

/** Chats, projects and model targets that can be inserted inline in a prompt. */
export function buildMentionGroups(kind?: Mention['kind']): ZCommandGroup<Mention>[] {
  const projectNames = new Map(
    workspace.projects.map((project) => [project.id, project.name]),
  );
  return [
  {
    heading: 'Chats',
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
    heading: 'Models',
    items: AGENTS.flatMap((entry) =>
      MODELS[entry.value].map((entryModel) => ({
        id: `ai:${entry.value}/${entryModel.value}`,
        label: mentionModelLabel(entry.label, entryModel),
        icon: agentIcon[entry.value],
        data: {
          kind: 'ai' as const,
          id: `${entry.value}/${entryModel.value}`,
          label: mentionModelLabel(entry.label, entryModel),
        },
      })),
    ),
  },
  {
    heading: 'Flows',
    items: allPresets().map((preset) => ({
      id: `workflow:${preset.id}`,
      label: preset.name,
      icon: 'lucideWorkflow' as ZIconName,
      keywords: [preset.description, ...preset.agents.map((agent) => agent.name)],
      data: { kind: 'workflow' as const, id: preset.id, label: preset.name },
    })),
  },
  ].filter((group) => !kind || (kind === 'session' ? group.heading === 'Chats' : kind === 'project' ? group.heading === 'Projects' : kind === 'workflow' ? group.heading === 'Flows' : group.heading === 'Models'));
}
