# Auto-switch accounts and provider API keys — implementation plan

## Goal and success criteria

Implement the approved design in [the design spec](../specs/2026-10-30-auto-account-provider-switch-design.md): on limit/auth failures, try another eligible connector account or custom-provider API key, update Settings and composer live, and continue the original work silently. If all candidates fail, restore the original credential and show the existing resume card without switching agents.

## Constraints and defaults

- The checkout is already dirty on `fix/chat-hang-history-projects`. Before editing, inspect and preserve all existing diffs, especially Copilot runtime/connector/settings changes and the prior ChatInput stop/resume work. Do not reset, stash, or stage unrelated files.
- Use the current `agentdeck` IndexedDB database. Add dedicated `accounts` and `providers` object stores; keep the existing `index` store for indexed listings.
- Keep connector authentication tokens in their existing native managers. Cache connector identities/status/quota snapshots in IndexedDB. Custom API keys remain local in IndexedDB, as they already are.
- Auto-switch is independently configurable per supported connector and custom provider. OpenCode has no account-profile manager and is out of account rotation scope; its Settings control must explain that limitation.
- Read quota cache from IndexedDB immediately to shortlist/rank candidates. Treat snapshots up to 60 seconds old as fresh; refresh older/incomplete candidate data before trying it. If the cache says no candidate has room, refresh all supported accounts once before declaring exhaustion. Cache never proves a credential works; verify after temporary activation and before committing the selected account.
- Progress is one AgentDeck command-style block, not a shell command. Elapsed time counts up from zero with no 30-second deadline. Wait a random 5–10 seconds before each candidate attempt.
- Only limit/quota and authentication failures trigger credential rotation. Transient failures retain current retry behavior. Auto-switch exhaustion bypasses cross-agent fallback and returns to the existing resume card.
- Do not add or run tests unless requested. Run the repository-required checks and build at the end.

## Ordered implementation steps

### 1. Protect and establish the working baseline

- Review the exact current diff in all files touched by the feature and record which edits predate this implementation.
- Run the relevant non-test baseline check if the dirty branch permits it; report pre-existing failures separately. Do not alter unrelated edits to make the baseline clean.
- Keep all feature edits in the current checkout so the uncommitted Copilot and composer work remains available.

### 2. Add IndexedDB stores and migrate current provider settings

- Extend `prefs-db.ts` with `accounts` and `providers` stores and route `accounts.*` and `providers.*` keys to them.
- Increment the IndexedDB schema version and move existing `providers.custom` data from `prefs` to `providers` inside the upgrade transaction.
- Keep existing index-cache keys in the `index` store unchanged. Confirm `dumpStores`/store validation include the new stores.
- Hydrate `providers.custom` through a compatibility migration: turn each legacy `apiKey`/`keyEnv` into the first key record and preserve the active key and all existing provider fields.
- Add typed account quota cache helpers keyed by connector/account identity, including observed time and status; hydrate cache before background refresh and use fresh cache to avoid unnecessary quota fetches.

### 3. Add provider key collections and selection

- Replace the single custom-provider key with named key records, active key id, and provider-level auto-switch configuration.
- Update provider form state to clone/migrate key arrays safely; add and remove keys, edit each secret, pick the active key, and test a specific key without losing the selected provider model.
- Update the provider list with the Auto switch badge and retain existing enabled, restricted, logo, model, and delete behavior.
- Resolve one active key for each custom chat request and carry its provider/key identity into completion/error handling without exposing the secret in events or logs.
- Cache provider quota per provider/key when available; use OpenRouter remaining quota for ranking and saved key order where quota is unavailable.

### 4. Add connector auto-switch settings and cached quota ranking

- Add persisted per-connector auto-switch settings for account-capable connectors and per-provider settings for custom providers.
- Rehydrate cached account/quota state immediately, then refresh live data when the account panel loads and before a rotation decision.
- Rank non-active, non-expired accounts by remaining usable quota across live windows. Place accounts without a per-account quota signal after ranked accounts, in saved order.
- Use manager refresh results to validate Claude/Codex profiles and their quota windows. For candidates whose manager exposes no inactive-account quota (Copilot/Antigravity), temporarily switch, then call the live Copilot usage endpoint or Antigravity quota/model endpoint as a bounded health check. Do not publish the candidate as selected or advance its account epoch until the health check passes.
- If a candidate fails, switch back to the original account and continue. Keep sends for that connector serialized during the attempt; refresh the actual active account after success or rollback.
- Write refreshed quota snapshots to IndexedDB and clear account cache entries when an account is removed.

### 5. Implement one recovery coordinator for accounts and API keys

- Intercept limit/quota and auth failures before the existing generic provider-recovery chain when auto-switch is enabled for the failing credential.
- Create or update one command-style progress block on the failed assistant reply. Show a masked account/key label, current phase, and elapsed seconds counting upward.
- Before each attempt, wait a random 5–10 seconds. Try connector candidates with temporary switch plus health check; try provider API keys with `provider_probe` and quota ordering.
- After a provider request itself returns limit/auth failure, mark that key unusable for the current recovery attempt and proceed to another key; do not rotate on transient errors.
- On success, commit the selected credential into the shared reactive state, update the Settings/composer display, increment the connector account epoch when applicable, finish the progress block, and send the hidden continuation prompt with the prior request/work context.
- On exhaustion, best-effort restore the original credential and refresh actual active state. Finish the progress block and show the existing resume card. For auth failures without a reset time, use manual recovery with auto-resume disabled. Do not fall back to another agent/provider in this path.
- Ensure cancellation, chat switching, account removal, and competing manual switches cannot leave the coordinator or active credential in a half-switched state.

### 6. Add Settings and composer controls

- Add a compact per-connector Auto switch toggle to supported account sections. Show a disabled explanation for connectors without a multi-account manager.
- Add the equivalent provider-level toggle to create/edit and keep the setting visible as a small badge in the provider list.
- Add a small Auto switch toggle and active account/key label below the composer. Bind it to the same per-connector/provider setting used in Settings, not a separate chat override.
- Animate active account/key label changes with a short fade and keep Settings/modal/footer state synchronized through shared reactive data.
- Ensure progress text and all key labels are masked; no full secret enters chat content, audit, or logs.

### 7. Verify and review

- Run `pnpm check` (workspace Cargo check/clippy plus package checks) and `pnpm --filter desktop build`.
- Inspect the final diff against the initial dirty baseline, confirm no unrelated files were staged or reverted, and report any live-account checks that could not be exercised.

## Migration and rollback notes

- The IndexedDB upgrade must move existing `providers.custom` atomically from `prefs` to `providers`; do not delete the old value until the destination write is part of the same upgrade transaction.
- Legacy provider-key conversion must be idempotent. If hydration or persistence fails, retain the readable legacy fields until the migrated provider list is saved successfully.
- Quota cache is advisory only. An expired/stale value can affect attempt ordering but cannot make an account appear verified.
- A failed temporary account switch must restore and re-read the original active account before the resume card is shown. If rollback itself fails, show the actual account reported by the manager and surface the error in the progress block.
- Preserve the initial uncommitted diff exactly outside files/hunks intentionally changed for this feature.
