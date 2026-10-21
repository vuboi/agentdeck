export type Agent = 'claude' | 'codex' | 'gemini';
export type Session = { id: string; title: string; agent: Agent; status: 'working' | 'idle' | 'waiting' };

export type Block =
  | { kind: 'thinking'; text: string; streaming?: boolean }
  | { kind: 'response'; text: string; streaming?: boolean }
  | { kind: 'command'; command: string; output: string; status: 'running' | 'ok' | 'fail' }
  | { kind: 'loading'; label: string };

export type Message = { id: string; role: 'user' | 'assistant'; time: string; blocks: Block[] };

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
