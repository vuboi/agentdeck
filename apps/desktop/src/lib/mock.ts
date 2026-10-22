/** A file a tool wrote: lines added and removed, and the text for a small diff. `create` once the result says so. */
export type FileChange = { path: string; added: number; removed: number; before: string; after: string; create?: boolean };

export type Agent = 'claude' | 'codex' | 'gemini';
export type Session = { id: string; title: string; agent: Agent; status: 'working' | 'idle' | 'waiting' };

export type Block =
  | { kind: 'thinking'; text: string; streaming?: boolean }
  | { kind: 'response'; text: string; streaming?: boolean }
  | { kind: 'command'; command: string; output: string; status: 'running' | 'ok' | 'fail'; id?: string; change?: FileChange }
  | { kind: 'loading'; label: string }
  | { kind: 'image'; src: string; name: string }
  | { kind: 'element'; tag: string; text: string; selector: string }
  | { kind: 'permission'; requests: { id: string; tool: string; summary: string; allow: string }[]; state: 'pending' | 'allowed' | 'denied' };

export type Message = {
  id: string;
  role: 'user' | 'assistant';
  time: string;
  blocks: Block[];
  /** ISO time of the message, for the relative time and its tooltip. */
  at?: string;
  /** Reply still being written by an agent. */
  live?: boolean;
  startedAt?: number;
  /** How long the reply took, once finished. */
  ms?: number;
};

export const sessions: Session[] = [
  { id: 's1', title: 'Build the settings panel', agent: 'claude', status: 'working' },
  { id: 's2', title: 'Review auth middleware', agent: 'codex', status: 'waiting' },
  { id: 's3', title: 'Explore component options', agent: 'gemini', status: 'idle' },
];

const longThinking =
  'The request is for a settings panel. Start from the structure: a navigation rail on the left, grouped preferences in the middle, and a save bar at the bottom. Keep the layout consistent with the rest of the workspace, use the existing tokens for color and spacing, and make sure the controls keep their focus states for keyboard users. Check the empty and error states before writing any markup.';

const longOutput = Array.from({ length: 14 }, (_, i) => `src/settings/section-${i + 1}.svelte  +${12 + i} -${i}`).join('\n');

export const messages: Message[] = [
  {
    id: 'm1',
    role: 'user',
    time: '10:42',
    blocks: [{ kind: 'response', text: 'Can you sketch a clean settings panel for the desktop app?' }],
  },
  {
    id: 'm2',
    role: 'assistant',
    time: '10:42',
    blocks: [
      { kind: 'thinking', text: longThinking },
      { kind: 'command', command: 'ls src/settings', output: longOutput, status: 'ok' },
      {
        kind: 'response',
        text: 'I will start with the structure: a focused navigation rail, grouped preferences, and clear save feedback. The right panel can show the live preview while we iterate. Next I will add the section components and wire the save bar to the store.',
      },
      { kind: 'command', command: 'pnpm --filter desktop check', output: '', status: 'running' },
      { kind: 'loading', label: 'Waiting for the type check' },
    ],
  },
];

// Enough turns to make virtualization visible: only the rows near the viewport are mounted.
const topics = ['sidebar spacing', 'terminal shortcuts', 'theme tokens', 'resize handles', 'chat blocks', 'quota card'];
for (let i = 1; i <= 80; i += 1) {
  const topic = topics[i % topics.length];
  messages.push(
    {
      id: `u${i}`,
      role: 'user',
      time: '11:0' + (i % 10),
      blocks: [{ kind: 'response', text: `Follow-up ${i}: can we tighten the ${topic}?` }],
    },
    {
      id: `a${i}`,
      role: 'assistant',
      time: '11:0' + (i % 10),
      blocks: [
        ...(i % 3 === 0 ? [{ kind: 'thinking' as const, text: `Checking how ${topic} is wired before changing it. `.repeat(6) }] : []),
        ...(i % 4 === 0
          ? [{ kind: 'command' as const, command: `rg "${topic.split(' ')[0]}" src`, output: `src/lib/${topic.replace(' ', '-')}.svelte:12`, status: 'ok' as const }]
          : []),
        { kind: 'response', text: `Done for ${topic}. The change is small and keeps the existing tokens, so nothing else moves.` },
      ],
    },
  );
}

// A reply with rich text and file changes, to see the markdown and activity styles without running an agent.
messages.push(
  { id: 'u-demo', role: 'user', time: '11:20', blocks: [{ kind: 'response', text: 'Show me what a full answer looks like.' }] },
  {
    id: 'a-demo',
    role: 'assistant',
    time: '11:20',
    at: new Date(Date.now() - 4 * 60_000).toISOString(),
    ms: 24_000,
    blocks: [
      { kind: 'command', id: 'c1', command: 'Bash  pnpm --filter desktop check', output: '0 errors, 0 warnings', status: 'ok' },
      {
        kind: 'command',
        id: 'c2',
        command: 'Write  src/lib/blocks.ts',
        output: 'File created successfully',
        status: 'ok',
        change: { path: 'src/lib/blocks.ts', added: 136, removed: 0, before: '', after: 'export const a = 1;\nexport const b = 2;', create: true },
      },
      {
        kind: 'command',
        id: 'c3',
        command: 'Edit  src/lib/app.css',
        output: '',
        status: 'ok',
        change: { path: 'src/lib/app.css', added: 3, removed: 1, before: 'color: red;', after: 'color: blue;\nmargin: 0;\npadding: 0;' },
      },
      { kind: 'permission', state: 'pending', requests: [{ id: 'p1', tool: 'Bash', summary: 'git push origin main', allow: 'Bash(git push origin main)' }] },
      {
        kind: 'response',
        text: '## Done\n\nHere is what changed:\n\n- **Blocks**: new `blocks.ts` with the helpers\n- Styles: a *softer* colour\n  - nested item\n- [Docs](https://example.com)\n\n1. First step\n2. Second step\n\n> A note worth keeping.\n\n```ts\nexport function add(a: number, b: number) {\n  return a + b;\n}\n```\n\n| File | Lines |\n| --- | --- |\n| blocks.ts | 136 |\n| app.css | 4 |\n',
      },
    ],
  },
);
