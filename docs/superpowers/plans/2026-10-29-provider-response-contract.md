# Provider response contract implementation plan

## Goal and acceptance criteria

Implement the approved design in `docs/superpowers/specs/2026-10-29-provider-response-contract-design.md` while preserving all unrelated working-tree edits.

- One user message always remains distinct and has a stable association to its reply/turn.
- Enter, Alt+Enter and Ctrl/Cmd+Enter retain their approved queue, priority-queue and stop-then-send semantics.
- Provider behavior is runtime-extensible; shared coordinator owns transcript and queue invariants, while each provider/connector owns raw transport semantics.
- No native steer or parallel mode is used unless each result can be associated with the correct user message.
- Child output, auto-turns, aborts and late events cannot leak into or close a newer root reply.
- Existing Claude, Codex, Antigravity, OpenCode, Copilot and custom-provider behavior has explicit strategy behavior; unknown connectors fall back safely.

## Constraints and assumptions

- Work in the existing dirty checkout on `main`; never revert or stage unrelated files.
- `PLAN.md` and `docs/development/adding-agent-provider.md` govern module order and DoD.
- Keep Copilot's SDK runtime separate from generic CLI process management while connecting it through the same response contract.
- Probe reports are evidence, not permanent guarantees. Zed and unmeasured connectors remain conservative until behavior is verified.
- Keep the approved composer controls and per-message reply presentation unchanged.

## Ordered implementation steps

### 1. Trace current ownership and capture boundaries

- Inspect only the relevant existing diffs in `sessions.svelte.ts`, `agent.ts`, `agent_runs.rs`, `copilot.rs`, `custom_chat.rs`, and `crates/adapters/src/agent/*` before editing.
- Map every send/stop/auto-turn path, including silent turns, workflow/delegate paths, permission continuation, context compression, and tail events.
- Record the existing event/session/turn IDs and find any caller of `agent_inject` still expected to steer.
- Preserve behavior outside response dispatch and list any conflicts found in the handoff doc.

### 2. Define a typed, runtime-resolved response contract

- Replace `ChatResponseCapabilities` booleans with typed delivery, interruption, terminal-event, child/background and scope descriptors.
- Put shared contract types in `crates/adapters/src/agent` and expose a provider method with a conservative custom default.
- Keep an executable strategy seam for send/stop/event mapping so Copilot can participate without implementing the generic CLI `Provider` trait.
- Make unsupported or unknown behavior explicit; do not infer support from provider IDs or model names.
- Add focused unit cases for default and per-provider strategy descriptors.

### 3. Wire the shared coordinator to message identity and delivery

- Give each outbound user message a stable ID through queued dispatch and the backend turn request/event envelope.
- Keep `runGroup` separate from message identity; each assistant response belongs to one outbound message unless it is explicitly an auto-turn.
- Make queue dispatch consult the active provider strategy; ordinary Enter/Alt+Enter follow-ups stay queued. Send-now stops the active interaction, waits for its provider-defined terminal acknowledgement/fallback, then dispatches its own message.
- Prevent Copilot's implicit inbox path from using `Immediate` steering for ordinary chat messages; preserve explicit supported operations only when the association invariant is met.
- Make turn completion/error/abort idempotent and keyed to the matching interaction so stale provider events cannot mutate a newer reply.
- Retain silent/internal turns, permission answers, workflow steps and delegated tasks without inventing visible user messages.

### 4. Integrate provider strategies incrementally

- Claude: preserve `result` boundary and tail auto-turn; ordinary follow-ups remain app-queued; keep scoped sub-agent/task linkage.
- Codex: filter root thread lifecycle; queue by default; gate native queue on persistent-thread eligibility; do not use steer for normal follow-ups.
- Antigravity: queue per interaction; kill-and-close stop fallback; retain `result` boundary and documented child/background limitations.
- Copilot: share contract types/semantics while retaining SDK-specific send, enqueue, abort-drain and `session.idle` logic; filter sub-agent events.
- OpenCode: app queue to prevent overlapping same-session processes; preserve dedupe and sub-agent linkage.
- Custom/OpenRouter and other connectors: AgentDeck-managed queue/turn loop; unknown providers use conservative defaults.
- Update `docs/development/adding-agent-provider.md` and `chat-response-layer-handoff.md` to reflect actual contract and per-provider status.

### 5. Verification and cleanup

- Add provider-specific fixture/conformance coverage for enqueue/dispatch, end, interruption, auto-turn, child scope, stale event, and per-message association.
- Run targeted adapter and frontend checks, then the module DoD commands: `cargo check -p agentdeck-desktop`, `cargo clippy -p agentdeck-desktop -- -D warnings`, and `pnpm --filter desktop check`; run adapter tests where affected.
- Inspect the final diff and `git status`; do not stage/commit implementation changes unless separately requested.
- Report build/test results separately from live authenticated provider verification. Do not claim real CLI success for providers that were not exercised.

## Risks and rollback points

- **Existing dirty changes overlap response code.** Review and preserve current hunks; make narrow edits and avoid broad formatting.
- **Provider completion semantics differ.** Keep per-provider terminal policies and conservative fallback; roll back only the affected provider strategy if its conformance case fails.
- **Turn/message identity touches Rust-to-TS event contracts.** Update both sides together and retain backward-compatible optional parsing for persisted/older events if the type is serialized to storage.
- **Copilot's SDK may continue emitting events after an abort or detached shell.** Keep its SDK-specific drain loop and prove it cannot close a later interaction before changing it.
- **Zed or another connector is unmeasured.** Leave it on app-managed queue until a real probe demonstrates safe native delivery.
