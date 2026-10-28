import { neutralTokens, primaryTokens, radiusTokens, type Mode, type Neutral, type Primary } from './theme-tokens';

const KEY = 'agentdeck.theme';
export type ChatLayout = 'split' | 'single';
type Saved = { primary: Primary; neutral: Neutral; mode: Mode; chatLayout: ChatLayout; turnDividers: boolean };
const DEFAULTS: Saved = { primary: 'violet', neutral: 'zinc', mode: 'dark', chatLayout: 'split', turnDividers: true };
// The corner radii live in app.css (--radius-small / --radius-large). `--radius` itself is what z-ui falls back to when
// something reads it directly: the large size.
const RADIUS = 0.375;

function read(): Saved {
  try {
    return { ...DEFAULTS, ...(JSON.parse(localStorage.getItem(KEY) ?? '{}') as Partial<Saved>) };
  } catch {
    return { ...DEFAULTS };
  }
}

const initial = read();
const media = matchMedia('(prefers-color-scheme: dark)');

/** Theme state. Tokens are applied inline on <html>, so they win over the stylesheet's z-theme layer. */
class Theme {
  primary = $state(initial.primary);
  neutral = $state(initial.neutral);
  readonly radius = RADIUS;
  mode = $state(initial.mode);
  /** `split`: your messages on the right, the agent on the left. `single`: everything on the left. */
  chatLayout = $state(initial.chatLayout);
  /** A thin line between two replies of the agent, and a line where a session was resumed or compacted (on by default). */
  turnDividers = $state(initial.turnDividers);
  private systemDark = $state(media.matches);

  readonly dark = $derived(this.mode === 'system' ? this.systemDark : this.mode === 'dark');
  private readonly tokens = $derived({
    ...neutralTokens(this.neutral, this.dark),
    ...primaryTokens(this.primary, this.dark),
    ...radiusTokens(this.radius),
  });

  constructor() {
    media.addEventListener('change', (event) => (this.systemDark = event.matches));
    // App-lifetime singleton: the root effect is never torn down.
    $effect.root(() => {
      $effect(() => {
        const root = document.documentElement;
        root.classList.toggle('dark', this.dark);
        for (const [name, value] of Object.entries(this.tokens)) root.style.setProperty(name, value, 'important');
        try {
          const saved: Saved = { primary: this.primary, neutral: this.neutral, mode: this.mode, chatLayout: this.chatLayout, turnDividers: this.turnDividers };
          localStorage.setItem(KEY, JSON.stringify(saved));
        } catch {
          // Persisting is best-effort.
        }
      });
    });
  }

  toggle() {
    this.mode = this.dark ? 'light' : 'dark';
  }
}

export const theme = new Theme();
