# Installation & build wiring

What a consumer project must contain for `@shival99/z-ui` to build and render correctly: the style
entries in `angular.json` / `project.json`, the PostCSS config, the providers, and which optional
npm packages each component pulls in.

`npx @shival99/z-ui-init init` writes all of this for you. This file is the reference for doing it
by hand, for reviewing a project that drifted, and for answering "why does the build ask for a
package I don't use".

## 1. Style entries (`angular.json` → `projects.<app>.architect.build.options.styles`)

Nx `project.json` uses the same `targets.build.options.styles` array.

```jsonc
"styles": [
  "node_modules/@shival99/z-ui/assets/css/tailwind.css", // required, must be first
  "src/styles.css"                                       // your own app styles, last
]
```

`tailwind.css` pulls in Tailwind itself, the z-ui base/animation/overlay layers and **every bundled
theme** — switching theme at runtime only flips the `z-theme` attribute, it never loads a CSS file.

### Feature CSS — add only what you use

Third-party global CSS is **not** in `tailwind.css`. A bundler has to resolve every `@import`
specifier before it can build, so bundling them would force every consumer to install `vidstack`
and `@univerjs/*` even when they never render a media player or an Excel preview
(`peerDependenciesMeta.optional` does not change that — it only silences the package manager).

| File                             | Add it when you use                                  | Packages it needs                                   |
| -------------------------------- | ---------------------------------------------------- | --------------------------------------------------- |
| `assets/css/features/media.css`  | `z-media-player`, or `z-gallery` audio/video preview | `vidstack`                                          |
| `assets/css/features/office.css` | `z-gallery` Excel preview                            | `@univerjs/presets`, `@univerjs/preset-sheets-core` |

```jsonc
"styles": [
  "node_modules/@shival99/z-ui/assets/css/tailwind.css",
  "node_modules/@shival99/z-ui/assets/css/features/media.css",  // only if used
  "node_modules/@shival99/z-ui/assets/css/features/office.css", // only if used
  "src/styles.css"
]
```

Symptom of a missing entry: the component works but renders unstyled (a bare `<video>`, an
unformatted sheet). Symptom of an entry you don't need: the build fails resolving a package you
never intended to install.

`npx @shival99/z-ui-init add <component>` re-syncs these entries from the registry, so prefer it
over editing by hand.

## 2. `.postcssrc.json` (project root)

```json
{
  "plugins": { "@tailwindcss/postcss": {} }
}
```

Tailwind v4 runs through PostCSS; without this file none of the utility classes are generated.

## 3. Providers (`app.config.ts`)

```ts
import { provideHttpClient } from '@angular/common/http';
import { provideZIconLoader } from '@shival99/z-ui/components/z-icon';
import { provideZToast } from '@shival99/z-ui/components/z-toast';
import { provideZCache, provideZIndexDb, provideZTheme, provideZTranslate } from '@shival99/z-ui/providers';

providers: [
  provideHttpClient(),
  provideZIconLoader(),
  provideZToast({ zPosition: 'top-right', zDuration: 3000, zCloseButton: true }),
  provideZTranslate({ defaultLang: 'vi' }),
  provideZTheme({ defaultTheme: 'neutral', defaultDarkMode: false }),
  provideZCache({ appName: 'my-app' }),
  provideZIndexDb({ appName: 'my-app' }),
];
```

## 4. `tsconfig.json`

Nothing to add. z-ui ships as an npm package with an `exports` map, so every entrypoint
(`@shival99/z-ui/components/z-button`, `@shival99/z-ui/services`, …) resolves through normal Node
resolution. Only add a `paths` entry if you are consuming the library from source inside a monorepo.

## 5. Entrypoints that carry optional dependencies

Importing an entrypoint makes its packages **mandatory at build time**, whether the import is
static or `await import()` — resolution happens before tree-shaking, so unused code still needs the
package on disk. Import the narrowest entrypoint you actually need.

| Entrypoint                                 | Requires                                                                                              |
| ------------------------------------------ | ----------------------------------------------------------------------------------------------------- |
| `@shival99/z-ui/services`                  | nothing beyond the base peers                                                                         |
| `@shival99/z-ui/services/excel`            | `exceljs`, `file-saver`, `xlsx`                                                                       |
| `@shival99/z-ui/services/office`           | `docx-preview`, `xlsx`, `@univerjs/presets`, `@univerjs/preset-sheets-core`, `@aiden0z/pptx-renderer` |
| `@shival99/z-ui/components/z-table`        | `@tanstack/angular-table`, `@tanstack/angular-virtual` + the `services/excel` set (Excel export)      |
| `@shival99/z-ui/components/z-media-player` | `vidstack`, `hls.js`, `dashjs` + `features/media.css`                                                 |
| `@shival99/z-ui/components/z-gallery`      | `ng2-pdfjs-viewer`, `shiki` + the media and office sets + both feature CSS files                      |
| `@shival99/z-ui/components/z-editor`       | the `@tiptap/*` set                                                                                   |
| `@shival99/z-ui/components/z-code`         | `shiki`                                                                                               |

`libs/z-ui-init/z-ui.registry.json` (shipped as `@shival99/z-ui-init/z-ui.registry.json`) is the
generated source of truth: every component's `dependencies` and `styles`, resolved through the
import graph. `npx @shival99/z-ui-init components --json` prints it, and `doctor` reports what a
project is missing.

## 6. Bundle budgets

The default Angular `initial` budget (500 kB / 1 MB) is far below a real z-ui app. Raise it in
`configurations.production.budgets` to what your app actually ships; `z-ui-init doctor` warns when
it still sees the legacy 4 MB / 6 MB pair.
