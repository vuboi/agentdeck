export type Agent = 'claude' | 'codex' | 'gemini';
export type Session = { id: string; title: string; agent: Agent; status: 'working' | 'idle' | 'waiting' };
export type Message = { id: string; role: 'user' | 'assistant'; content: string; time: string };

export const sessions: Session[] = [
  { id: 's1', title: 'Build the settings panel', agent: 'claude', status: 'working' },
  { id: 's2', title: 'Review auth middleware', agent: 'codex', status: 'waiting' },
  { id: 's3', title: 'Explore component options', agent: 'gemini', status: 'idle' },
];

export const messages: Message[] = [
  { id: 'm1', role: 'user', content: 'Can you sketch a clean settings panel for the desktop app?', time: '10:42' },
  { id: 'm2', role: 'assistant', content: 'I’ll start with the structure: a focused navigation rail, grouped preferences, and clear save feedback. The right panel can show the live preview while we iterate.', time: '10:42' },
];
