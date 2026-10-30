// The user's way of talking, learnt quietly from their own messages and shared by every agent in AgentDeck, so each
// one answers in the language, tone and form of address the user uses. Heuristic only (no model call, nothing shown in
// the chat): each main-chat prompt adds to running counts kept in localStorage, and the note an agent gets at the start
// of a conversation is built from them. Pure apart from the storage helpers, so node can test it.

const KEY = 'agentdeck.userProfile';
/** Below this many prompts the counts say too little: no note yet. */
export const PROFILE_MIN_PROMPTS = 5;
/** Old habits fade: past this many prompts every count is scaled down, so the profile follows the user as they change. */
const WINDOW = 200;

export type UserProfile = {
  prompts: number;
  chars: number;
  /** Prompts by language guess. */
  languages: Record<string, number>;
  /** How the user refers to themselves / to the AI (Vietnamese pronoun pairs and English). */
  pronouns: Record<string, number>;
  casual: number;
  jokes: number;
  polite: number;
  blunt: number;
  noDiacritics: number;
  delegation: number;
  reviews: number;
  updatedAt: number;
};

export const emptyProfile = (): UserProfile => ({ prompts: 0, chars: 0, languages: {}, pronouns: {}, casual: 0, jokes: 0, polite: 0, blunt: 0, noDiacritics: 0, delegation: 0, reviews: 0, updatedAt: 0 });

/** The user's own words: without the `[AgentDeck] ...` paragraph and the inline context the app puts before a prompt. */
export function ownWords(prompt: string): string {
  let text = prompt.replace(/^\[AgentDeck\][\s\S]*?\n\n/, '');
  const cut = text.lastIndexOf('\n---\n');
  if (cut >= 0 && /^The user included/.test(text)) text = text.slice(cut + 5);
  return text.trim();
}

const VI_LETTERS = /[ăâđêôơưạ-ỹ]/i;
const fold = (text: string) => text.normalize('NFD').replace(/[\u0300-\u036f]/g, '').replace(/đ/gi, 'd').toLowerCase();

/** A rough language guess from the script of the text. */
export function guessLanguage(text: string): string {
  if (VI_LETTERS.test(text)) return 'Vietnamese';
  if (/[\u3040-\u30ff]/.test(text)) return 'Japanese';
  if (/[\uac00-\ud7af]/.test(text)) return 'Korean';
  if (/[\u4e00-\u9fff]/.test(text)) return 'Chinese';
  if (/[\u0400-\u04ff]/.test(text)) return 'Russian';
  if (/[\u0e00-\u0e7f]/.test(text)) return 'Thai';
  const vi = fold(text).match(/\b(khong|duoc|nhe|nha|di|cai|nay|lam|sua|ban|toi|minh|roi|thi|la|cho|voi)\b/g)?.length ?? 0;
  if (vi >= 2) return 'Vietnamese';
  return /[a-z]/i.test(text) ? 'English' : 'other';
}

const PRONOUNS: [string, RegExp][] = [
  ['tao/mày', /\b(tao|may|m|mi)\b/],
  ['tôi/bạn', /\b(toi|ban)\b/],
  ['mình/bạn', /\b(minh)\b/],
  ['anh/em', /\b(anh|em)\b/],
  ['bro/ông', /\b(bro|ong|ae)\b/],
];

/** Adds one prompt to the profile (a new object). */
export function observe(profile: UserProfile, prompt: string, now = Date.now()): UserProfile {
  const text = ownWords(prompt);
  if (!text) return profile;
  const next: UserProfile = structuredClone(profile);
  if (next.prompts >= WINDOW) {
    const scale = (value: number) => value * 0.9;
    for (const key of ['prompts', 'chars', 'casual', 'jokes', 'polite', 'blunt', 'noDiacritics', 'delegation', 'reviews'] as const) next[key] = scale(next[key]);
    for (const map of [next.languages, next.pronouns]) for (const key of Object.keys(map)) map[key] = scale(map[key]);
  }
  const folded = fold(text);
  const language = guessLanguage(text);
  next.prompts += 1;
  next.chars += Math.min(text.length, 2000);
  next.languages[language] = (next.languages[language] ?? 0) + 1;
  for (const [name, pattern] of PRONOUNS) if (pattern.test(folded)) next.pronouns[name] = (next.pronouns[name] ?? 0) + 1;
  if (/\b(oke|ok|uk|uh|ko|k|dc|vs|j|hok|nhe|nha|thoi|bro|lol)\b/.test(folded)) next.casual += 1;
  if (/(haha|hihi|kk+|=\)+|:\)+|:v|xd|lol|🤣|😂|😆|😄)/i.test(text)) next.jokes += 1;
  if (/\b(please|thanks|thank you|cam on|lam on|vui long|xin)\b/.test(folded)) next.polite += 1;
  if (/\b(dm|vl|vcl|wtf|deo|dell|ngu)\b/.test(folded)) next.blunt += 1;
  if (language === 'Vietnamese' && !VI_LETTERS.test(text)) next.noDiacritics += 1;
  if (/\b(sub ?agent|giao cho|delegate|@\w)/.test(folded)) next.delegation += 1;
  if (/\b(review|kiem tra lai|check lai)\b/.test(folded)) next.reviews += 1;
  next.updatedAt = now;
  return next;
}

const share = (part: number, whole: number) => (whole > 0 ? part / whole : 0);

/** The note for the start of a conversation, or '' while too little is known. */
export function profileHint(profile: UserProfile): string {
  if (profile.prompts < PROFILE_MIN_PROMPTS) return '';
  const total = profile.prompts;
  const traits: string[] = [];
  const languages = Object.entries(profile.languages).filter(([name]) => name !== 'other').sort((a, b) => b[1] - a[1]);
  if (languages[0]) traits.push(`writes mostly in ${languages[0][0]} (${Math.round(share(languages[0][1], total) * 100)}% of prompts)`);
  const pronoun = Object.entries(profile.pronouns).sort((a, b) => b[1] - a[1])[0];
  if (pronoun && share(pronoun[1], total) >= 0.15) traits.push(`addresses the AI with "${pronoun[0]}"; mirror that register naturally`);
  const average = profile.chars / total;
  traits.push(average < 120 ? 'short, terse prompts (often with typos): infer intent, answer concisely' : 'detailed prompts');
  if (share(profile.casual, total) >= 0.3) traits.push('casual, informal tone');
  if (share(profile.jokes, total) >= 0.1) traits.push('jokes around: light humour is welcome');
  if (share(profile.blunt, total) >= 0.05) traits.push('blunt wording: stay direct, do not take offence');
  if (share(profile.polite, total) >= 0.3) traits.push('polite, formal tone');
  if (share(profile.noDiacritics, profile.languages.Vietnamese ?? 0) >= 0.3) traits.push('often types Vietnamese without diacritics; reply with proper diacritics');
  if (share(profile.delegation, total) >= 0.1) traits.push('likes handing work to sub-agents');
  if (share(profile.reviews, total) >= 0.1) traits.push('expects work to be reviewed before it is called done');
  return `User communication profile (learnt from their past messages across AgentDeck; match their tone and form of address; the language of their current message still wins): ${traits.join('; ')}.`;
}

export function loadProfile(): UserProfile {
  try {
    return { ...emptyProfile(), ...(JSON.parse(localStorage.getItem(KEY) ?? '{}') as Partial<UserProfile>) };
  } catch {
    return emptyProfile();
  }
}

export function saveProfile(profile: UserProfile): void {
  try {
    localStorage.setItem(KEY, JSON.stringify(profile));
  } catch {
    // Best effort.
  }
}

export function clearProfile(): void {
  try {
    localStorage.removeItem(KEY);
  } catch {
    // Best effort.
  }
}

/** Learns from one main-chat prompt; never throws. */
export function learnFromPrompt(prompt: string): void {
  try {
    saveProfile(observe(loadProfile(), prompt));
  } catch {
    // Learning is best-effort.
  }
}
