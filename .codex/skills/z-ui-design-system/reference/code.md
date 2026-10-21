# Code — `z-code`

Selector: `z-code`. A syntax-highlighted code block (powered by Shiki) with an optional title bar — file icon,
mono filename, icon-only copy button — falling back to a copy button floating over the code. Never build a code block by hand with `<pre><code class="language-*">`.

```html
<z-code zTitle="hello.component.ts" [zCode]="tsExample()" zLanguage="typescript" />
```

`zCode` can also be omitted and the code passed as projected content (a `<pre><code>` in the default slot) —
the component reads `textContent` off the DOM after view init and highlights that instead. Prefer passing
`zCode` explicitly; it's simpler and reactive to changes.

No CVA size/variant file — `z-code` has no `zSize`/`zType` variant classes, so there is no size/variant table
to document.

## Inputs

| Input              | Type                      | Default        | Notes                                                                                                                                                                                                                                                                                   |
| ------------------ | ------------------------- | -------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `class`            | `ClassValue`              | `''`           | Extra Tailwind classes, merged with the wrapper's classes                                                                                                                                                                                                                               |
| `zTitle`           | `string`                  | `''`           | Header bar title text (e.g. a filename). If empty, no header renders and the copy button floats top-right over the code instead                                                                                                                                                         |
| `zLanguage`        | `BundledLanguage` (Shiki) | `'typescript'` | Syntax highlighting language, e.g. `'javascript'`, `'html'`, `'css'`, `'scss'`, `'json'`, `'bash'`, `'shell'`, `'markdown'`, `'yaml'`, `'python'`, `'java'`, `'sql'`, `'xml'`, `'angular-ts'`, `'angular-html'` (bundle only includes these; other Shiki languages are not pre-bundled) |
| `zCode`            | `string`                  | `''`           | The source code to highlight. If left empty, the component falls back to reading projected `<pre><code>` content from the DOM                                                                                                                                                           |
| `zTheme`           | `ZCodeTheme \| ''`        | `''`           | Pins one highlight theme; see table below. If empty, **both** palettes are rendered at once and CSS swaps them — see below                                                                                                                                                              |
| `zShowLineNumbers` | `boolean`                 | `true`         | Shows the line-number gutter                                                                                                                                                                                                                                                            |
| `zHighlightLines`  | `readonly number[]`       | `[]`           | 1-based line numbers to band with a tinted background and a rule down the leading edge                                                                                                                                                                                                  |
| `zScrollable`      | `boolean`                 | `false`        | Caps the body height and scrolls it instead of letting the block grow. The title header stays put while the code scrolls under it                                                                                                                                                       |
| `zMaxHeight`       | `number`                  | `400`          | That cap, in pixels. Only applied when `zScrollable` is set                                                                                                                                                                                                                             |

No outputs — copying is handled internally (shows a toast and swaps the button to a 2s check icon); the
copy button is icon-only in both the header and the floating variant. There is no `zCopied` output to
listen for.

## Theming: leave `zTheme` empty

With no `zTheme`, the block is highlighted with Shiki's dual-theme mode
(`themes: { light: 'github-light', dark: 'github-dark' }`). Shiki writes the light colours inline and the
dark ones as `--shiki-dark` custom properties, and the component's stylesheet swaps between them under
`.dark`. Switching colour mode is then a repaint, not a fresh asynchronous highlight of every block on the
page — which is what it used to be when the theme was picked from `ZThemeService.isDark()` in TypeScript.

Set `zTheme` only to pin one palette regardless of colour mode.

| Value            | Notes                                    |
| ---------------- | ---------------------------------------- |
| `'github-light'` | The light half of the dual-theme default |
| `'github-dark'`  | The dark half of the dual-theme default  |
| `'vitesse-dark'` |                                          |
| `'one-dark-pro'` |                                          |

## Line highlighting

`zHighlightLines` is applied by a Shiki transformer during highlighting (`zCodeHighlightLines` in
`z-code.utils.ts`), which marks each requested line's node with `line-highlighted`. That is the only
reliable way to target a line: the rendered markup carries no line index of its own, so there is nothing to
select against afterwards. Don't try to band lines with `:nth-child` on the output.

## Examples

```html
<!-- Titled code block with filename header -->
<z-code zTitle="hello.component.ts" [zCode]="tsExample()" zLanguage="typescript" />

<!-- No title: copy button floats top-right over the code -->
<z-code [zCode]="bashExample()" zLanguage="bash" />

<!-- JSON snippet -->
<z-code zTitle="package.json" [zCode]="jsonExample()" zLanguage="json" />

<!-- Pin a theme regardless of app colour mode -->
<z-code zTitle="query.sql" [zCode]="sqlExample()" zLanguage="sql" zTheme="one-dark-pro" />

<!-- Band the lines that matter (1-based) -->
<z-code zTitle="counter.ts" [zCode]="tsExample()" [zHighlightLines]="[4, 5]" />

<!-- Long file: scroll the body instead of growing the page -->
<z-code zTitle="rows.ts" [zCode]="longExample()" [zScrollable]="true" [zMaxHeight]="220" />

<!-- No gutter -->
<z-code [zCode]="bashExample()" zLanguage="bash" [zShowLineNumbers]="false" />

<!-- Extra wrapper classes -->
<z-code zTitle="styles.css" [zCode]="cssExample()" zLanguage="css" class="border-primary/40" />
```

## Composing an install command

A package-manager switcher over a command line needs no new component. `z-tabs` with
`zType="segment"` gives the muted track
with a gliding raised pill. The brand marks cannot go through `ZTab.icon` — that only names an icon from
a bundled `@ng-icons` set (heroicons / iconoir / iconsax / lucide / phosphor / tabler, none of which carry
package-manager marks), and `z-icon` paints a single `currentColor` glyph while these are multi-colour
SVG. They ride in through `ZTab.iconTemplate` instead, and because that template receives the tab as its
implicit context, **one** template serves the whole strip:

```html
<ng-template #packageManagerIcon let-tab>
  <my-brand-icon [zName]="tab.value" />
</ng-template>

<z-tabs
  [zTabs]="packageManagerTabs()"
  zType="segment"
  zSize="sm"
  [zTabsOnly]="true"
  [activeTab]="activePackageManager()"
  (zTabChange)="setPackageManager($event)"
/>
<div class="border-border bg-background flex h-10 items-center gap-2 rounded-sm border px-3">
  <code class="grow truncate font-mono text-sm">{{ installCommand() }}</code>
  <button z-button zType="ghost" zSize="xs" zIconOnly="true" (click)="copyInstallCommand()">
    <z-icon [zType]="installCopied() ? 'lucideCheck' : 'lucideCopy'" zSize="14" />
  </button>
</div>
```

The command is a plain bordered row, not a `z-code` — a single line of text does not need a `<pre>` and
its padding, and the row carries its own copy so nothing has to be reshaped to fit.

```ts
protected readonly packageManagerIcon = viewChild<TemplateRef<unknown>>('packageManagerIcon');
protected readonly packageManagerTabs = computed<ZTab[]>(() => {
  const iconTemplate = this.packageManagerIcon();
  return this.packageManagers.map(name => ({ label: name, value: name, iconTemplate }));
});
```

Don't hand-roll the switcher out of `z-button`s to get the icons in — that trades away the glide for
nothing now that `iconTemplate` exists.

## Language tabs

Same idea, one level up: `z-tabs` picks the language and `z-code` renders the matching file, with its
own filename header and a scrolling body. Only languages present in `z-code.utils.ts` can be offered —
`tsx`/`jsx` are **not** in the bundle, so a "React" tab needs them added there first.

Use `zType="underline-hover"`, not `line`: `line` fades each tab's own `::after` bar in place, while
`underline-hover` glides one shared underline between tabs (and adds the pointer highlight). And pass no
horizontal padding on the `z-tabs` element — a static `class` lands on the host, which insets the strip
_and_ the hairline its `.z-tabs::after` draws, so the rule stops short of the container's side borders.
The tabs carry their own `px-3` already.

```html
<z-tabs
  [zTabs]="languageTabItems"
  zType="underline-hover"
  zSize="sm"
  [zTabsOnly]="true"
  [activeTab]="activeCodeLanguage()"
  (zTabChange)="setCodeLanguage($event)"
  class="bg-muted/50 w-full"
/>
<z-code
  [zCode]="activeLanguageTab().code"
  [zLanguage]="activeLanguageTab().language"
  [zTitle]="activeLanguageTab().filename"
  [zScrollable]="true"
  [zMaxHeight]="280"
/>
```

## Anti-patterns

```html
<!-- WRONG: raw pre/code, no syntax highlighting, no copy button, no theme integration -->
<pre><code class="language-typescript">{{ tsExample() }}</code></pre>

<!-- WRONG: invented prop, there is no zReadOnly on z-code -->
<z-code zReadOnly="true" [zCode]="tsExample()"></z-code>

<!-- WRONG: invented size prop, z-code has no size/variant CVA -->
<z-code zSize="sm" [zCode]="tsExample()"></z-code>

<!-- WRONG: capping height with a class — the body still grows, the wrapper just clips it -->
<z-code [zCode]="longExample()" class="max-h-56 overflow-y-auto" />
<!-- RIGHT -->
<z-code [zCode]="longExample()" [zScrollable]="true" [zMaxHeight]="224" />
```
