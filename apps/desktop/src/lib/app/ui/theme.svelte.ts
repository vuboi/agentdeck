import { neutralTokens, primaryTokens, radiusTokens, type Mode, type Neutral, type Primary } from './theme-tokens';

const KEY = 'agentdeck.theme';
export type ChatLayout = 'split' | 'single';
/** The text size of the whole app, in px: the root size every rem (Tailwind's text sizes) follows. */
export const UI_FONT_SIZE = { min: 11, max: 20, step: 1, default: 16 } as const;
type Saved = { primary: Primary; neutral: Neutral; mode: Mode; chatLayout: ChatLayout; turnDividers: boolean; fontSize: number };
const DEFAULTS: Saved = { primary: 'violet', neutral: 'zinc', mode: 'dark', chatLayout: 'split', turnDividers: true, fontSize: UI_FONT_SIZE.default };
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
  /** The app's text size in px (see UI_FONT_SIZE). */
  uiFontSize = $state(Math.min(UI_FONT_SIZE.max, Math.max(UI_FONT_SIZE.min, initial.fontSize)));
  private systemDark = $state(media.matches);

  readonly dark = $derived(this.mode === 'system' ? this.systemDark : this.mode === 'dark');
  /**
   * The theme as heavy views (the chat's rows) should see it: it follows `dark`/`primary`/`neutral` two painted frames
   * later, so the new colours show at once and the rebuild of those views comes after instead of holding the swap.
   */
  settledKey = $state('');
  private readonly tokens = $derived({
    ...neutralTokens(this.neutral, this.dark),
    ...primaryTokens(this.primary, this.dark),
    ...radiusTokens(this.radius),
  });

  constructor() {
    media.addEventListener('change', (event) => (this.systemDark = event.matches));
    this.settledKey = this.keyNow();
    let settle = 0;
    let unfreeze = 0;
    // App-lifetime singleton: the root effect is never torn down.
    $effect.root(() => {
      $effect(() => {
        const root = document.documentElement;
        const key = this.keyNow();
        const changed = key !== this.settledKey;
        // During a theme swap nothing animates: thousands of `transition-colors` elements easing at once on a busy
        // page is what left the window half in the old theme and half in the new one.
        if (changed) {
          root.classList.add('theme-switching');
          cancelAnimationFrame(unfreeze);
        }
        root.classList.toggle('dark', this.dark);
        root.style.colorScheme = this.dark ? 'dark' : 'light';
        root.style.fontSize = `${this.uiFontSize}px`;
        for (const [name, value] of Object.entries(this.tokens)) root.style.setProperty(name, value, 'important');
        if (changed) {
          cancelAnimationFrame(settle);
          // First frame paints the new colours; then heavy views rebuild, and transitions come back a frame later.
          settle = requestAnimationFrame(() => {
            settle = requestAnimationFrame(() => {
              this.settledKey = key;
              unfreeze = requestAnimationFrame(() => root.classList.remove('theme-switching'));
            });
          });
        }
        try {
          const saved: Saved = { primary: this.primary, neutral: this.neutral, mode: this.mode, chatLayout: this.chatLayout, turnDividers: this.turnDividers, fontSize: this.uiFontSize };
          localStorage.setItem(KEY, JSON.stringify(saved));
        } catch {
          // Persisting is best-effort.
        }
      });
    });
  }

  private keyNow() {
    return `${this.dark}|${this.primary}|${this.neutral}`;
  }

  toggle() {
    this.mode = this.dark ? 'light' : 'dark';
  }
}

export const theme = new Theme();
