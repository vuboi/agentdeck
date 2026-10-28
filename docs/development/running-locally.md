# Running AgentDeck beside an installed copy

Use the root scripts below instead of invoking `tauri dev` directly. `scripts/run-desktop.mjs` keeps the native identity, Vite URL, CSP and debug Preview capability in agreement.

| Command | Purpose | Native identity | Frontend |
| --- | --- | --- | --- |
| `pnpm dev` | Native development with hot reload | `dev.agentdeck.desktop.dev` | Loopback Vite server, preferred port 1431 |
| `pnpm dev:web` | Browser-only frontend development | No native app/backend | A separate free loopback port |
| `pnpm dev:update` | Native dev with a simulated update dialog | Same Dev identity | Same launcher and port selection as `dev` |
| `pnpm run:build` | Build and open a local production-mode app | `dev.agentdeck.desktop.local` | Embedded files; no Vite server |
| `pnpm build` / `pnpm build:win` | Produce normal installers | `dev.agentdeck.desktop` | Embedded files |

## Isolation

- Installed **AgentDeck**, **AgentDeck Dev** and **AgentDeck Local** have different single-instance mutexes and native app-data directories. This separates their WebView profiles, bridge tokens and process registries. A second instance of the same native channel still activates its existing window.
- Dev checks its preferred Vite port and the next 20 ports without terminating or reusing an unrelated listener. It prints the chosen URL. Override the preferred port with `VITE_DEV_PORT`. Vite, Tauri `devUrl`, dev CSP and the debug Preview capability use the same chosen origin. `strictPort` remains enabled so a late port race cannot silently point the app at another server.
- The browser MCP bridge prefers 47821 for the installed app, 47822 for Dev and 47823 for Local, with an OS-assigned free port as fallback.
- `run:build` writes native outputs under `target/local`, uses `--no-bundle`, does not create installers/updater artifacts, and does not install over the normal app. The first build may be slow because this target directory has its own compilation cache. Close a previously running **AgentDeck Local** before rebuilding that executable on Windows.
- Real application updater checks/installations are disabled in Dev and Local; `dev:update` remains a simulation. Normal release behavior is unchanged.
- Agent CLI authentication, global skills/MCP files and other explicitly user-level `~/.agentdeck` resources are still shared. These commands isolate the app runtime, not provider accounts or project working files.
- Frontend IndexedDB is origin-scoped. A dev fallback to another port has a different frontend storage origin. Use the normal preferred port consistently when you want the same development chat/preferences database.

## Inspect without launching

Each run command accepts `--dry-run` to print its resolved configuration and selected port without building or starting another app/server. For example:

```powershell
pnpm dev --dry-run
pnpm dev:update --dry-run
pnpm run:build --dry-run
```

Port probing is advisory: if another process takes the selected port immediately afterwards, startup fails visibly rather than killing that process. Keep existing user terminals running during investigation; do not solve a port conflict by blindly stopping the listener.
