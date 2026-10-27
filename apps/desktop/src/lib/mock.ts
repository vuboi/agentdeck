/** A file a tool wrote: lines added and removed, and the text for a small diff. `create` once the result says so. */
export type FileChange = { path: string; added: number; removed: number; before: string; after: string; create?: boolean };

export type Agent = 'claude' | 'codex' | 'antigravity' | 'opencode' | 'custom';
export type Session = { id: string; title: string; agent: Agent; status: 'working' | 'idle' | 'waiting' };

export type Block =
  | { kind: 'thinking'; text: string; streaming?: boolean }
  | { kind: 'response'; text: string; streaming?: boolean }
  | { kind: 'command'; command: string; output: string; status: 'running' | 'ok' | 'fail'; id?: string; /** A delegate_task call: the task it started (see delegate-task.ts). */ taskId?: string; change?: FileChange; /** A screenshot the tool returned, as a data: URL. */ image?: string }
  | { kind: 'loading'; label: string }
  /** Part of the user's message handed to another agent in a chat of its own; `state` follows that chat. */
  | { kind: 'delegate'; childId: string; agent: Agent; model: string; state: 'queued' | 'running' | 'asking' | 'done' | 'failed' | 'skipped'; note?: string; /** The workflow step it runs, when it comes from a flow. */ step?: string; /** Questions the step asked the user (state 'asking'). */ questions?: import('./questions').Question[]; /** The earlier step it is asking something right now, and what. */ talking?: string; talkText?: string }
  | { kind: 'image'; src: string; name: string }
  /** A document attached to the message: it stays on disk, the agent is given its path. */
  | { kind: 'file'; name: string; path: string; size?: number }
  | { kind: 'element'; tag: string; text: string; selector: string; /** What the chat keeps of the element: for its card, and to send it again when the message is edited. */ url?: string; title?: string; note?: string; html?: string; around?: string; box?: { x?: number; y?: number; w: number; h: number }; style?: Record<string, string>; /** A PNG of the element, for its chip. */ picture?: string }
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
  /** Tokens the reply used so far (all its model calls), and the model it ran on, for the cost estimate. */
  tokens?: import('./usage').Tokens;
  model?: string;
  /** The output count of the model call in progress (reports are totals per call). */
  callOutput?: number;
  /** Tokens the last model call of the turn read (its input with the cache): how full the context is. */
  context?: number;
  /** The reply of an auto-compress (`/compact`) turn. */
  compressed?: boolean;
  /** Exact cost reported by the CLI when the turn ended. */
  cost?: number;
  /** The agent that wrote this reply. A chat can switch agents, so each reply keeps its own. */
  agent?: Agent;
};

export const sessions: Session[] = [
  { id: 's1', title: 'Build the settings panel', agent: 'claude', status: 'working' },
  { id: 's2', title: 'Review auth middleware', agent: 'codex', status: 'waiting' },
  { id: 's3', title: 'Explore component options', agent: 'antigravity', status: 'idle' },
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
        id: 'c1b',
        command: 'mcp__agentdeck__browser_screenshot  {}',
        output: '',
        status: 'ok',
        image:
          'data:image/svg+xml;utf8,' +
          encodeURIComponent('<svg xmlns="http://www.w3.org/2000/svg" width="640" height="400"><rect width="640" height="400" fill="#0b1220"/><rect x="24" y="24" width="592" height="40" rx="8" fill="#1e293b"/><text x="40" y="52" fill="#34d399" font-family="sans-serif" font-size="20">Z-UI · Command</text><rect x="24" y="96" width="280" height="160" rx="10" fill="#1e293b"/><rect x="336" y="96" width="280" height="160" rx="10" fill="#1e293b"/></svg>'),
      },
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
      {
        kind: 'command',
        id: 'c5',
        command: 'mcp__agentdeck__delegate_task  {"agent":"claude","model":"haiku","task":"Search the repo for usages of useSession and list the files that call it."}',
        output: '[task t-demo1 · chat claude:demo-worker · Claude · Haiku 5.5 · done · 42s]\n\nSTATUS: done\nRESULT: 4 call sites, all read-only\nFILES: src/a.ts; src/b.ts; src/c.ts; src/d.ts\nOPEN: none',
        status: 'ok',
      },
      { kind: 'command', id: 'c6', command: 'agentdeck.delegate_task  {"agent":"codex","task":"Review the diff of the last commit for bugs.","wait":false}', output: '', status: 'running' },
      { kind: 'permission', state: 'pending', requests: [{ id: 'p1', tool: 'Bash', summary: 'git push origin main', allow: 'Bash(git push origin main)' }] },
      {
        kind: 'response',
        text: '## Done\n\nHere is what changed:\n\n- **Blocks**: new `blocks.ts` with the helpers\n- Styles: a *softer* colour\n  - nested item\n- [Docs](https://example.com)\n\n1. First step\n2. Second step\n\n> A note worth keeping.\n\n```ts\nexport function add(a: number, b: number) {\n  return a + b;\n}\n```\n\n| File | Lines |\n| --- | --- |\n| blocks.ts | 136 |\n| app.css | 4 |\n',
      },
    ],
  },
);
