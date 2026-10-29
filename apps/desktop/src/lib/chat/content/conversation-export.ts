import type { Message, Block } from '../mock';

/** Converts the complete rendered conversation into a portable Markdown transcript. */
export function conversationMarkdown(title: string, messages: Message[], sourcePath?: string): string {
  const lines = [`# ${title || 'Conversation'}`, '', `Exported from AgentDeck on ${new Date().toISOString()}.`];
  if (sourcePath) lines.push('', `Source transcript: \`${sourcePath}\``);
  for (const message of messages) {
    lines.push('', `## ${message.role === 'user' ? 'User' : message.agent ?? 'Assistant'}${message.at ? ` · ${message.at}` : ''}`, '');
    for (const block of message.blocks) {
      const text = blockText(block);
      if (text) lines.push(text, '');
    }
  }
  return `${lines.join('\n').trim()}\n`;
}

function blockText(block: Block): string {
  switch (block.kind) {
    case 'response': return block.text;
    case 'thinking': return block.text ? `> **Thinking**\n> ${block.text.replaceAll('\n', '\n> ')}` : '';
    case 'command': {
      const parts = [`**Tool (${block.status}):** \`${block.command}\``];
      if (block.output) parts.push(`<details><summary>Tool output</summary>\n\n\`\`\`text\n${block.output}\n\`\`\`\n</details>`);
      if (block.change) parts.push(`**File change:** \`${block.change.path}\` (+${block.change.added} / −${block.change.removed})`);
      return parts.join('\n\n');
    }
    case 'delegate': return `**Delegated to ${block.agent} (${block.state}):** ${block.note ?? ''}`;
    case 'image': return `**Image:** ${block.name}`;
    case 'file': return `**File:** ${block.name} — \`${block.path}\``;
    case 'element': return `**Page element:** ${block.tag}${block.text ? ` — ${block.text}` : ''}${block.url ? ` (${block.url})` : ''}${block.note ? `\n\n${block.note}` : ''}`;
    case 'permission': return block.requests.map((request) => `**Permission ${block.state}:** ${request.tool} — ${request.summary}`).join('\n');
    case 'loading': return '';
  }
}
