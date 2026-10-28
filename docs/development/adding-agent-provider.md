# Adding a new agent/provider

Checklist for the next coding agent. Read `AGENTS.md` and `PLAN.md` before editing. Use the current GitHub Copilot integration as an example of a provider SDK authenticated through another CLI; do not copy its auth/MCP approach when the new provider has its own SDK or CLI.

## Principles

- Choose a stable, lowercase provider ID, for example `copilot`. Use the same ID across Rust, Svelte, session storage, workflows, MCP, and skills.
- Find and extend the existing registries, types, and lists; do not create a second picker, logo registry, model cache, connector, or skill manager.
- A provider may need several layers: login, account switching, runtime, and model discovery. Adding a Settings card alone is not a complete integration.
- A chat provider is not automatically an image-generation provider. Distinguish the model used to compose a prompt from the backend that actually generates the image.
- Preserve the user's existing workspace changes; do not reset or recursively format files outside the scope.
- Each surface below often has its own `if agent === ...` branch or handwritten agent list. Search for the new ID (`grep -rn "'claude'" apps/desktop/src`) so no surface is missed.
- Before writing API calls, check how the provider's official client works (the vendor's open-source repository on GitHub). Use the same endpoint and headers; do not invent headers based on another client.

## Known Copilot pitfalls

- **Chat is blocked:** `sessions.svelte.ts` has an agent list used by `startTurn`. A provider missing from that list falls into the `"<agent> chats are not connected yet"` branch even when the backend already has `agent_send`. Add the new ID to the correct list.
- **Quota bar is missing:** `Composer.svelte` (`const quota = $derived.by`) only handles agents with an explicit branch. Add a branch that reads `connectors.accounts[<id>]` (`usage`, `observed_at`) and ensure that provider's `accounts_list` fills `usage` for the active account.
- **Quota does not refresh:** the five-minute check in `connectors.svelte.ts` (`setInterval(check, 5 * 60_000)`) refreshes only listed providers. Add the provider if threshold warnings need it.
- **Window units:** `UsageWindow.pct` is the percentage used, while the quota bar shows the remaining percentage (`100 - pct`). Window labels must match `^\d+(m|h|d)$` (for example `30d`) so the nearest-to-limit window can be calculated.
- **CLI commands are blocked:** `account_tool_allows` (`connectors.rs`) permits only selected `codex-auth`/`cswap` commands. Other CLIs (for example `gh api`) are not blocked, but new tools still require explicit review.

## Surface checklist

### 1. ID, logo, and agent information

- Add the profile to `apps/desktop/src/lib/provider-registry.ts` (`BuiltInAgent`, `BUILT_IN_AGENTS`). This is the shared source for logos, names, icons, and colors in chat, workflows, and pickers.
- Add the agent to any independent unions/types: `sessions.svelte.ts`, `mock.ts`, slash-command types, and connector types as needed.
- Review handwritten color maps in `Chat.svelte`, `Sidebar.svelte`, `SettingsModal.svelte`, and the MCP view. Prefer `AgentLogo`/the registry over adding generic icons.
- AgentDeck stores the provider on each chat session; check labels and logos in recent chats, the header, tabs, and task cards. Do not confuse new AgentDeck sessions with CLI history imports.

### 2. Login, accounts, and Settings

- Backend connector in `apps/desktop/src-tauri/src/connectors.rs`: binary detection, `Login`, login status, login/cancel, account list/add/switch, and clear error messages.
- Frontend in `apps/desktop/src/lib/connectors.svelte.ts`: update `ConnectorId`, `AccountProvider`, `Detected`, state, refresh, and switching actions. Never put tokens or token-bearing output in the UI/log.
- Add a connector card in `components/SettingsModal.svelte` with binary, logo, description, login, and an account manager when the provider supports multiple accounts.
- When the account changes, call `accountSwitched(providerId)` so chat does not resume with stale credentials; reload the model list if the catalog depends on the account.
- If the CLI is the chosen login method, read credentials from the CLI only when needed, pass them directly to the SDK, and never store tokens in preferences, source, or logs.

### 3. Chat runtime, sessions, and model discovery

- If the SDK runtime does not use the `Provider` CLI/stdout protocol, add a separate branch in `apps/desktop/src-tauri/src/agent_runs.rs` before `provider_for`, using the existing `TurnRequest`, stop tracking, and AgentDeck event envelope.
- Translate SDK events into the existing `AgentEvent`: delta, thinking, tool call/result, usage, permission, done/error. Clean up the session/client after completion, error, or cancellation.
- If the SDK/provider includes diffs in arguments, map them to `AgentEvent::ToolCall.change` (`FileChange`: path, +/- counts, before/after) so the wrapper can show them as soon as the tool starts.
- If the provider sends only a path, the adapter may snapshot the file before the tool and send a diff in `AgentEvent::ToolResult.change` after completion. Reuse `begin_file_snapshot` / `finish_file_snapshot`: read only small text files inside the project root; skip binaries, large files, and paths outside the root. Do not use a Git workspace snapshot as a fallback because it would mix in changes from before the chat turn.
- The chat aggregates `FileChange` blocks in a message into the `Edited N files, +X −Y` wrapper and preview. Keep the blocks in the same message so AgentDeck stores them in the existing history; do not create a provider-specific panel or store.
- Use the provider's own session ID for resume; do not create a new session on every turn. Verify that switching accounts starts a new session.
- Connect `models_list` in `src-tauri/src/lib.rs` to the provider's model discovery. In `models.svelte.ts`, update `ProviderId`, catalog, source, `modelsReady`, and `loadModels`.
- Reload models after login/switch. If the provider is not signed in or its catalog cannot be loaded, return a controlled error/empty list without breaking another provider's picker.
- Workflows use models from `modelSources()` (`components/flow/FlowEditor.svelte`); verify that the new provider can be selected, saved, and run in a workflow.

### 4. Chat input, files/images, and Image Generator

- Chat input calls `agent_send`; verify that model, effort, permission mode, allowed approvals, stop/cancel, file context, and session resume reach the correct runtime.
- If the composer encodes images/files as paths in the prompt, convert those paths to native SDK attachments when supported; keep the displayed text/context unchanged for history.
- `ModelPicker.svelte` uses `modelSources()`; verify that the provider appears in the composer and `@AI` routing.
- `ImageGenerator.svelte` has a model for **composing the prompt** and a separate backend for **generating the image**. A new chat provider may be a prompt model when compatible; add it as an image-generation backend only after verifying a real image-generation API.
- If the model picker structure or UI changes, update `ModelPicker.svelte` and `ImageGenerator.svelte` together according to `AGENTS.md`.

### 5. MCP and Store

- Update `McpAgent` in `src/lib/mcp.svelte.ts`, the provider list in `components/McpView.svelte`, and the `AGENTS` allowlist in `src-tauri/src/mcp_manage.rs`.
- If the provider uses an SDK, pass shared MCP through the SDK session configuration; add separate config read/write when Store needs to enable/disable servers for that provider. Preserve other entries and never overwrite a malformed config file.
- Confirm that `servers_for(project)` returns only MCP servers matching the project/provider and that the SDK receives the correct stdio/http, env, and header schema.
- Review AgentDeck's internal browser MCP: if the provider needs browser tools, pass the bridge into the session and update the corresponding agent list; do not assume the user's MCP config already contains the bridge.
- Check add, list/merge, test, remove, and disabled state. Update the UI description if the provider stores configuration differently from the CLI's `mcp add`.

### 6. Skills and Store

- Read the [skill location conventions](store-skill-locations.md). Add the provider to `store/library.rs::SKILL_AGENTS`, `skill_folders` (global), and `project_skill_folders` (project); install/sync/remove must use the shared mapping.
- Update the Store registry and provider picker (`provider-registry.ts`, `SkillProviders.svelte`); check both global and project scope, and update the path table in the skill-location document.
- Add skill reads to `slash_commands.rs` and the allowed roots of `slash_commands::read`, so `/skill` sees the correct skills without opening files outside permitted directories.
- Enable SDK/runtime skill discovery and pass the folder when the provider requires explicit configuration. Check the user folder, project folder, and junction paths created by Store.
- Update the provider filter in Store Discover when the catalog contains provider-specific content.

## Current Copilot integration — SDK example

- Profile/logo: `provider-registry.ts`, with the `GITHUB_SVG` brand icon in `brand-icons.ts`.
- Auth/account: Settings calls the GitHub CLI; the active token comes from `gh auth token`, and switching uses `gh auth switch`. The SDK receives the token in memory.
- Runtime/model/chat: `src-tauri/src/copilot.rs`; runtime branch in `agent_runs.rs`; model listing in `lib.rs`.
- Attachments: composer lines such as `Attached image(s)` and `Attached file(s)` become SDK file attachments. These are chat input images, not an image-generation API.
- MCP: Store saves Copilot configuration at `~/.copilot/mcp-config.json`; Rust passes the shared configuration into the SDK session.
- Skills: Store assigns them to `~/.copilot/skills`; the slash palette reads provider skills and the project's `.github/skills`.
- Do not add Copilot to Claude/Codex quota warnings or the image backend when Copilot does not provide the corresponding data/capability.

## Handoff checks

- `cargo check -p agentdeck-desktop`
- `cargo clippy -p agentdeck-desktop -- -D warnings`
- `pnpm --filter desktop check`
- Run the real app (`pnpm dev`, or reload the window if HMR did not fully apply): sign in, open the model picker, send a short message, and check the response and quota bar. A green build does not prove that chat works.
- If the user requests testing, add/run suitable checks; do not call the network from tests. Authenticated runtime login/chat must be verified separately with an account that can use the provider.
- Report which surfaces were connected, which checks actually ran, and which runtime limits could not be verified. Do not claim login/chat success without a real turn.
