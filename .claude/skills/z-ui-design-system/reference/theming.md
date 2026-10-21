# Theming and theme configuration

Z-UI themes are CSS-variable palettes selected by the root `z-theme` attribute and combined with the root
`.dark` class. Import the bundled Tailwind entrypoint once; it includes the base tokens, all theme CSS, overlay
styles, animations, and the Tailwind v4 mappings.

## Installation

```json
"styles": [
  "node_modules/@shival99/z-ui/assets/css/tailwind.css",
  "src/styles.scss"
]
```

Do not add one CSS entry per theme. The old runtime theme-file loading path is deprecated because every theme is
already bundled.

## `provideZTheme`

Configure the service in `app.config.ts`:

```ts
import { provideZTheme } from '@shival99/z-ui/providers';

export const appConfig: ApplicationConfig = {
  providers: [
    provideZTheme({
      defaultTheme: 'green',
      defaultDarkMode: false,
      cacheKey: 'MY_APP_THEME_PREFERENCES',
    }),
  ],
};
```

| Option            | Type         | Default     | Behavior                                                                            |
| ----------------- | ------------ | ----------- | ----------------------------------------------------------------------------------- |
| `defaultTheme`    | `ZThemeName` | `'neutral'` | Used when no cached preference exists                                               |
| `defaultDarkMode` | `boolean`    | `true`      | Used when no cached preference exists                                               |
| `cacheKey`        | `string`     | library key | Storage key for `{ theme, isDark }`; blank/whitespace falls back to the library key |

`ZThemeName` is the union `green | orange | violet | neutral | stone | zinc | gray | slate | hospital`.
The initializer's `--theme` flag writes `defaultTheme` and the generated `z-ui.json` metadata; it does not create
a separate theme stylesheet.

## Runtime control

```ts
import { ZThemeService } from '@shival99/z-ui/services';

export class SettingsComponent {
  private readonly theme = inject(ZThemeService);

  useViolet(): void {
    this.theme.setTheme('violet');
  }

  setDark(isDark: boolean): void {
    this.theme.toggleDarkMode(isDark);
  }
}
```

The service exposes readonly signals `currentTheme` and `isDark`. On initialization it reads the configured cache
key, validates the theme name, applies `z-theme="<name>"`, and toggles `.dark`. Cached values take precedence over
provider defaults. `preloadTheme()` remains only as a backwards-compatible no-op; do not add calls for new code.

## CSS variable customization

Use semantic variables after the library stylesheet when you need an app-specific palette or radius:

```css
:root {
  --primary: oklch(0.6 0.2 240);
  --primary-foreground: oklch(1 0 0);
  --radius: 0.5rem;
}

.dark {
  --primary: oklch(0.7 0.2 240);
}
```

The base and bundled theme default for `--radius` is `0.375rem`. Tailwind derives `--radius-xs` through
`--radius-2xl` from it; see [tokens.md](./tokens.md) for the formulas and shadow variables. Use semantic utilities
such as `bg-primary`, `bg-popover`, `text-muted-foreground`, and `border-border` in component markup so the
override works in both modes.

The documentation site's `ZThemeCustomizerService` is app-specific. Its controls for primary/neutral color,
radius, color mode, icon set, and font write CSS variables/preferences directly; those controls are not part of the
published `provideZTheme` configuration API.

## Anti-patterns

```ts
// WRONG: manually loads a theme file at runtime; all themes are already bundled.
this.theme.preloadTheme('violet');
```

```html
<!-- WRONG: raw palette colors do not follow the selected theme or dark mode. -->
<div class="bg-blue-100 text-gray-800">...</div>

<!-- RIGHT -->
<div class="bg-primary text-primary-foreground">...</div>
```
