// Ported from the z-ui Svelte demo theme customizer: palette -> CSS variable tokens.
export type Primary =
  | 'black' | 'red' | 'orange' | 'amber' | 'yellow' | 'lime' | 'green' | 'emerald' | 'teal'
  | 'cyan' | 'sky' | 'blue' | 'indigo' | 'violet' | 'purple' | 'fuchsia' | 'pink' | 'rose';
export type Neutral = 'slate' | 'gray' | 'zinc' | 'neutral' | 'stone';
export type Mode = 'light' | 'dark' | 'system';

export const PRIMARIES: Primary[] = [
  'black', 'red', 'orange', 'amber', 'yellow', 'lime', 'green', 'emerald', 'teal',
  'cyan', 'sky', 'blue', 'indigo', 'violet', 'purple', 'fuchsia', 'pink', 'rose',
];
export const NEUTRALS: Neutral[] = ['slate', 'gray', 'zinc', 'neutral', 'stone'];
export const RADII = [0, 0.125, 0.25, 0.375, 0.5];

const NEAR_WHITE = 'oklch(0.985 0 0)';
const PURE_WHITE = 'oklch(1 0 0)';
const shade = (neutral: Neutral, step: number) => `var(--color-${neutral}-${step})`;
// Hairline dividers: softer than the raw palette step.
const hairline = (value: string) => `color-mix(in oklab, ${value} 65%, transparent)`;

export function neutralTokens(neutral: Neutral, dark: boolean): Record<string, string> {
  if (!dark) {
    return {
      '--background': PURE_WHITE,
      '--foreground': shade(neutral, 950),
      '--card': PURE_WHITE,
      '--card-foreground': shade(neutral, 950),
      '--popover': PURE_WHITE,
      '--popover-foreground': shade(neutral, 950),
      '--secondary': shade(neutral, 100),
      '--secondary-foreground': shade(neutral, 900),
      '--muted': shade(neutral, 100),
      '--muted-foreground': shade(neutral, 500),
      '--accent': shade(neutral, 100),
      '--accent-foreground': shade(neutral, 900),
      '--border': hairline(shade(neutral, 200)),
      '--input': shade(neutral, 200),
      '--sidebar': shade(neutral, 50),
      '--sidebar-foreground': shade(neutral, 950),
      '--sidebar-accent': shade(neutral, 100),
      '--sidebar-accent-foreground': shade(neutral, 900),
      '--sidebar-border': hairline(shade(neutral, 200)),
    };
  }
  return {
    // Keep dark mode charcoal instead of near-black. The 900 canvas and softer
    // card surface preserve hierarchy without making the whole app feel black.
    '--background': shade(neutral, 900),
    '--foreground': NEAR_WHITE,
    '--card': `color-mix(in oklab, ${shade(neutral, 900)} 72%, ${shade(neutral, 800)})`,
    '--card-foreground': NEAR_WHITE,
    '--popover': `color-mix(in oklab, ${shade(neutral, 900)} 55%, ${shade(neutral, 800)})`,
    '--popover-foreground': NEAR_WHITE,
    '--secondary': shade(neutral, 800),
    '--secondary-foreground': NEAR_WHITE,
    '--muted': shade(neutral, 800),
    '--muted-foreground': shade(neutral, 400),
    '--accent': shade(neutral, 800),
    '--accent-foreground': NEAR_WHITE,
    '--border': 'oklch(1 0 0 / 7%)',
    '--input': 'oklch(1 0 0 / 15%)',
    '--sidebar': shade(neutral, 900),
    '--sidebar-foreground': NEAR_WHITE,
    '--sidebar-accent': shade(neutral, 800),
    '--sidebar-accent-foreground': NEAR_WHITE,
    '--sidebar-border': 'oklch(1 0 0 / 7%)',
  };
}

const DARK_INK = 'oklch(0.141 0.005 285.823)';
const LIGHT_FOREGROUND = new Set<Primary>(['yellow', 'amber', 'lime']);

export function primaryTokens(primary: Primary, dark: boolean): Record<string, string> {
  const value = primary === 'black' ? (dark ? NEAR_WHITE : DARK_INK) : `var(--color-${primary}-${dark ? 400 : 500})`;
  const foreground =
    primary === 'black' ? (dark ? DARK_INK : NEAR_WHITE) : LIGHT_FOREGROUND.has(primary) ? DARK_INK : NEAR_WHITE;
  return {
    '--primary': value,
    '--primary-foreground': foreground,
    '--ring': value,
    '--sidebar-primary': value,
    '--sidebar-primary-foreground': foreground,
    '--sidebar-ring': value,
  };
}

export const radiusTokens = (radius: number): Record<string, string> => ({ '--radius': `${radius}rem` });

export function swatch(name: Primary | Neutral): string {
  return name === 'black' ? DARK_INK : `var(--color-${name}-500)`;
}
