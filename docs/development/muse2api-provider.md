# muse2api as a provider (note)

Source: https://github.com/www222fff/muse2api (Python, FastAPI, MIT). Gateway: exposes muse.ai chat, text-to-image and text-to-video as an OpenAI-compatible API.

## Facts from the repo

- Listens on `http://127.0.0.1:18610` by default (`MUSE2API_PORT`). OpenAI base URL: `http://127.0.0.1:18610/v1`.
- Every `/v1/*` route needs `Authorization: Bearer <key>`. Keys: `m2a-...`, created on the dashboard (`/dashboard`) or `POST /admin/keys`, shown once. Legacy `MUSE2API_API_KEY` also accepted.
- `GET /v1/models` returns three models with a `kind` field: `muse-chat` (chat), `muse-image` (image), `muse-video` (video), plus aliases (`gpt-4o`, `gpt-4o-mini`, `gpt-5`, `claude-sonnet-4`, `dall-e-3`, `sora`, ...). Aliases have `alias_of`, and an alias has the same `kind` as its target.
- Chat: `POST /v1/chat/completions`, streaming and non-streaming. Stateless: each request opens a new muse.ai thread and sends the full history. A stable `user` field keeps one thread.
- Driver: `MUSE2API_DRIVER=mock` (default, offline fake data) or `browser` (Chromium + CDP with muse.ai cookies). Real use needs `browser` plus imported account cookies.
- No tool calls, no sub-agents, no background work in the chat response (text only).

## Why it is not in the "On this computer" scan

- `local_scan` (`src-tauri/src/connectors.rs`, `local_scan`) probes `GET {base}/models` with no key. muse2api answers 401 without a key, so the scan never finds it. It is added as a preset instead, the same way `freeCloud` presets are.

## Where it plugs in

- `apps/desktop/src/lib/agents/endpoints.json` → new `gateways` list (address, note, key page). Read by TS (`endpoints.ts`) and by Rust only through `endpoints.rs` (Rust ignores unknown fields).
- `apps/desktop/src/lib/components/settings/sections/provider-form.svelte.ts` → `useGateway(preset)` prefills a custom provider (`kind: openai`, `baseUrl`, `defaultModel: muse-chat`).
- `apps/desktop/src/lib/components/settings/sections/ProvidersSection.svelte` → "Self-hosted gateways" card with Add / Added (same pattern as the free cloud cards).
- `apps/desktop/src-tauri/src/connectors.rs` → `parse_models`: keep only `kind == "chat"` items when a `kind` is given, so image and video models do not show in the chat picker. Tests updated.

## To do

- [x] Read repo: README, `.env.example`, `/v1/models`, auth (`api/deps.py`), model kinds (`core/models.py`)
- [x] Map AgentDeck surfaces (`endpoints.json`, provider form, Providers section, `parse_models`)
- [x] Add `gateways` to `endpoints.json` and `endpoints.ts`
- [x] `useGateway` in `provider-form.svelte.ts`
- [x] Gateway card in `ProvidersSection.svelte`
- [x] `parse_models` filter by `kind` + unit test
- [ ] Run `cargo test -p agentdeck-desktop connectors` (and `cargo clippy -p agentdeck-desktop -- -D warnings`)
- [ ] Run `pnpm --filter desktop check` (svelte-check, no errors)
- [ ] Live test: `python -m muse2api` (mock driver), create key on dashboard, Add in Settings, Test lists `muse-chat`, send a short chat
- [ ] Haiku review of the diff
- [ ] Remove temp files, clone at `/tmp/m2a` is outside the repo (no repo junk)

## Open choices

- `restricted: true` (Free tier badge: never gets project data) is set by default because the upstream is muse.ai and its data policy is unknown. Change it only after checking muse.ai's terms.
- Image and video models are hidden from chat. Image generation through muse2api (`/v1/images/generations`) is not wired; it needs the ImageGenerator backend rules in `docs/development/adding-agent-provider.md` section 4.
- Ban risk: the browser driver uses a muse.ai account's cookies and automation. Keep account use manual; do not auto-switch accounts (see memory `no-auto-account-switch`).
