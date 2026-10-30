# Auto-switch accounts and provider API keys

## Status

Design approved in chat on 2026-10-30. Implementation has not started.

## Problem

When a connector account or custom provider API key reaches a usage limit, AgentDeck currently retries/falls back according to the existing recovery chain or shows the limit-resume card. It does not rotate to another account or API key within the same connector/provider. Settings can hold multiple connector accounts, but each custom provider has one API key.

## Goals

- Rotate to another usable account for supported connectors, or another API key for a custom provider, when the current credential hits a limit or is invalid.
- Let users enable auto-switch independently for each connector and each custom provider.
- Show one AgentDeck-generated command-style progress block in the chat while candidates are checked. This is status UI, not a shell command. Its elapsed timer counts upward from zero and continues until success or all candidates are exhausted; it has no 30-second deadline.
- Wait a random 5–10 seconds before each candidate attempt.
- Verify each candidate. Connector accounts are temporarily switched to and tested; a failed candidate is rolled back before trying the next. Provider API keys can be probed without selecting them first.
- On success, update the selected account/key in Settings and the composer immediately, show a light fade, then continue the original work through a hidden prompt in a new conversation context.
- If no candidate works, show the existing resume card immediately; auto-resume follows the existing reset eligibility and user choice. Do not fall back to another agent/provider.
- Preserve existing account and provider data, including the current single API key.

## Non-goals

- Adding multi-account management to connectors that do not currently expose account profiles. Account rotation applies where AgentDeck has an account manager; unsupported connector controls should explain why they cannot rotate.
- Changing the existing recovery chain for failures when auto-switch is disabled.
- Rotating credentials on transient network/server errors that are not authentication or limit errors.
- Sending full API keys to the chat timeline, logs, or audit output.

## Recovery flow

1. Classify the finished error. If it is a limit/quota or authentication failure and auto-switch is enabled for the active connector/provider, start a credential-rotation operation before the existing exhausted-limit UI.
2. Append/update one command-style progress block in the current chat reply. Show elapsed time increasing from zero, the masked account/key label being checked, and a current state such as waiting, checking, succeeded, or failed. Never display the API key value.
3. Refresh available quota/status where supported. Exclude expired accounts and candidates with no live quota remaining. Rank candidates by known remaining quota, highest first; place candidates without a per-account quota signal after ranked candidates in their saved order.
4. Before each candidate attempt, wait a random 5–10 seconds. Temporarily switch connector credentials, run a bounded connection check, and commit the new selected account only on success. On failure, switch back to the original account and continue. Serialize new runs for that connector until the operation commits or rolls back.
5. For custom providers, probe each API key independently. Use reported quota for OpenRouter when available; otherwise try configured keys in list order. If a real model request returns a limit/auth error, mark that key unusable for this recovery attempt and rotate to the next key.
6. On success, update the shared reactive connector/provider settings state, increment the connector account epoch where applicable, complete the progress block, and send a hidden “Continue working” prompt that carries forward the original request and work already completed. Do not add an empty or synthetic user message.
7. If every candidate fails, restore the original credential, finish the progress block as failed, and show the existing resume card. For a quota failure, preserve the card's current auto/manual choice and reset eligibility. For an authentication failure with no known reset time, show it as a manual recovery state with auto-resume disabled. Do not move to another agent/provider in this exhaustion path.

Transient failures continue through the existing retry policy. When auto-switch is off, current recovery behavior remains unchanged.

## Settings and composer UI

- Add an independent auto-switch toggle to each supported connector account section and each custom provider's create/edit form.
- Add a compact “Auto switch” toggle below the composer. It reflects and changes the setting for the connector/provider selected in the current chat; it is not a separate per-chat override.
- Show a small Auto switch badge on enabled custom providers in the provider list.
- Use one reactive source of truth for the active account/key so the composer footer and Settings modal update together. Fade the active label/badge when it changes.
- Keep current manual account selection and provider enabled/disabled controls.

## Data and migration

- Persist auto-switch configuration per connector and per custom provider.
- Extend the app's existing IndexedDB database (`agentdeck`) with dedicated `accounts` and `providers` object stores. Route account cache keys to `accounts` and provider definitions/keys to `providers`; keep the existing `index` object store for indexed listings.
- Cache connector account identity/status and quota snapshots by connector and account id, including an observation timestamp. Rehydrate from IndexedDB immediately for candidate ranking, then refresh live state before attempting a switch. A stale cache may prioritize candidates but never proves that a credential works.
- Replace `CustomProvider.apiKey` with a list of named key records (`id`, label, secret, optional environment key name) and an active key id, plus the provider's auto-switch setting. Cache per-key provider quota snapshots by provider id and key id when the endpoint exposes them.
- Upgrade the IndexedDB schema and move `providers.custom` from the current `prefs` store into `providers`. During hydration/migration, convert a legacy `apiKey` and `keyEnv` into the first key record and choose it as active. Preserve provider id, model list, base URL, logo, enabled state, and restricted flag.
- Connector authentication secrets remain in their existing native account managers; IndexedDB stores account IDs and quota/status metadata, not copied access/refresh tokens. Custom API keys remain local in IndexedDB as they are today, and are never rendered in full or included in progress text.

## Quota and connector limits

- Claude and Codex account lists can provide per-account usage and expired status; use those figures to rank candidates.
- Copilot and Antigravity currently expose account status but not reliable quota for inactive accounts. Test candidates by temporarily switching and checking the live connection; rank unknown-quota accounts in saved order.
- OpenCode currently has no account-profile switcher in AgentDeck. It cannot participate in connector-account rotation until one exists.
- Provider quota lookup currently supports OpenRouter only. Other providers can be tested for authentication/model access, but their remaining usage cannot be ranked in advance; rotate after a real limit response.

## Acceptance criteria

- Toggling auto-switch for one connector/provider does not change any other toggle; the footer toggle and Settings toggle stay synchronized.
- A supported connector limit error shows an elapsed command-style status, observes the random delay, tests candidates, rolls back failures, and commits the first usable account.
- A custom provider keeps multiple API keys, preserves a legacy key on migration, probes candidate keys, and rotates after a limit/auth response.
- A successful rotation updates Settings and composer active labels without reload and fades the change, then continues the task without a visible continuation prompt.
- Exhausting candidates restores the original credential and immediately shows the existing resume card without cross-agent fallback; auto-resume remains available only when the failure has a known reset condition.
- Transient errors and auto-switch-disabled flows keep their existing behavior.

## Implementation areas

- `apps/desktop/src/lib/app/platform/prefs-db.ts` and `prefs.ts`: IndexedDB schema upgrade, new stores, key routing, and migration.
- `apps/desktop/src/lib/agents/connectors.svelte.ts` and `apps/desktop/src-tauri/src/connectors.rs`: config, cached account/quota hydration, account candidate refresh/check/switch, custom-provider key data/probe.
- `apps/desktop/src/lib/chat/session/events.svelte.ts`, `sending.svelte.ts`, and `run.svelte.ts`: recovery coordinator, progress block, key-specific provider request, hidden continuation.
- `apps/desktop/src/lib/components/settings/sections/ConnectorsSection.svelte`, `ProvidersSection.svelte`, `ProviderModals.svelte`, and `provider-form.svelte.ts`: settings toggles and API-key editor.
- `apps/desktop/src/lib/components/chat/ComposerDock.svelte`, `ComposerQuota.svelte`, and related composer components: active account/provider key display and mirrored toggle.
- Run the relevant desktop checks after implementation; no live credential values may appear in diagnostics or progress output.
