# Provider response contract and chat continuity

**Date:** 2026-10-29

**Status:** Proposed design, approved in chat for specification; awaiting review of this document before implementation.

## Goal

Make AgentDeck's transcript predictable while supporting providers with different message delivery, interruption, turn-end, child-agent, and background-task behavior. Keep the user's existing controls:

- Enter adds a distinct message to the end of the queue.
- Alt+Enter adds a distinct message to the front of the queue, after the active response.
- Ctrl+Enter / Cmd+Enter stops the active response and sends the selected message now.
- Each user message remains separately visible with its own associated assistant response. Follow-ups sent while work is active can share a visual run group, but their text and answers are not concatenated into one bubble.

The contract must be extensible at provider/connector level. Common code handles only lifecycle and behavior that can be represented without erasing meaningful provider differences.

## Current evidence and gaps

- `crates/adapters/src/agent/mod.rs` has `ChatResponseCapabilities`, currently three booleans for mid-turn injection, parallel messages, and auto-background turns. The desktop send path does not consume this contract.
- Claude, Codex, Antigravity, and OpenCode implement the trait method. Copilot has a separate Tauri runtime and its own inbox/steering behavior; it is not represented by an adapter capability implementation.
- Copilot's current run path sends inbox messages as native `Immediate` steering, which merges them into the active interaction. This conflicts with the transcript invariant above unless the user explicitly chooses a steer action and the provider can correlate the resulting answer.
- Existing UI queue code dispatches one queued message at a time and records `runGroup`; it should remain the transcript owner rather than relying on providers to merge user prompts.
- Real CLI probes and local histories are summarized in `docs/development/probe-reports/`. They show incompatible semantics: Claude/Codex steering can merge work; Antigravity queues; Copilot distinguishes enqueue from immediate; OpenCode can run overlapping requests. Turn-end and child-task correlation also vary.
- Probe coverage does not establish Zed-specific chat history or connector behavior. Treat Zed and future connectors as unmeasured until probed.

## Approaches considered

1. **Provider-only behavior:** each integration owns its queue, lifecycle, and transcript updates. This preserves flexibility but duplicates UI state and has already allowed inconsistent message/reply association.
2. **Shared orchestration contract with provider strategies (recommended):** one coordinator owns message identity, queue order, run grouping, and exactly-once turn closure. Each provider strategy declares and implements its delivery, interruption, event mapping, and completion semantics. Reusable policy helpers are shared only where evidence shows equivalent behavior.
3. **One forced behavior for all providers:** route every message through one generic queue/steer pattern. This appears simple but either ignores useful native features or silently changes their semantics.

## Design

### 1. Ownership boundaries

**Shared response coordinator** owns:

- One durable/transcript user message per send, plus a stable message ID.
- Queue ordering and dispatch of one queued item at a time.
- The association between each user message, provider interaction/turn, and assistant reply.
- A `runGroup` for visually continuous work, separate from the message/reply association.
- Idempotent completion, stop, and error transitions so duplicate or late provider events cannot close a newer turn.
- Rendering normalized lifecycle events and scoped child output without appending child text to the root answer.

**Provider/connector strategy** owns:

- Native send mode and how to submit one message: app-managed queue, provider queue, explicit steer, or parallel execution.
- Stop/abort mechanism and the terminal acknowledgement to wait for.
- How raw provider events map to normalized message, tool, child, background, usage, and terminal events.
- How to identify root versus child events, and whether background work yields a later automatic answer.
- Runtime-specific capability choices where behavior depends on session type, auth, model, SDK version, or connector mode.

The shared coordinator may provide small reusable strategies/helpers (for example, queue-and-dispatch or interrupt-and-wait-for-idle), but integrations can override any operation. Do not make provider behavior inherit from a provider-name hierarchy merely because two integrations currently look alike.

### 2. Contract shape

Replace the three independent booleans with a typed response strategy/descriptor consumed by the coordinator. Keep the exact Rust API open for implementation planning, but it must represent these dimensions without implying unsupported guarantees:

- **Delivery:** app queue; native enqueue; native steer with an explicit answer-association guarantee; or parallel with explicit independent-interaction IDs.
- **Interruption:** supported operation, whether the process/session remains reusable, and the event/acknowledgement that confirms closure.
- **Turn completion:** provider-specific end marker and any later event that begins an automatic turn. A model-round end must not be confused with session idle or task completion.
- **Child/background work:** correlation keys, parent tool linkage, child event source, completion signal or known absence of one, and whether it can produce a later root answer.
- **Runtime scope/version:** optional values resolved for the active session/connector rather than assumed globally.

The provider strategy is also an executable boundary (send/stop/map), not just descriptive metadata. A capability declaration must not claim that a behavior is safe unless the integration can preserve the association required by the transcript.

### 3. Normalized lifecycle and transcript invariants

Normalize raw events into a small provider-neutral lifecycle with stable IDs, while retaining provider metadata for diagnostics. At minimum distinguish:

- message accepted/queued/dispatched;
- interaction or turn started;
- root assistant content and tool activity;
- scoped child/task progress;
- automatic follow-up turn;
- completed, interrupted, failed, or provider-lost terminal state.

Each dispatched user message has an explicit association key. If a native steer merges two user messages into one provider answer and cannot report which message it answered, the coordinator must queue it instead for normal Enter/Alt+Enter behavior. A user-selected immediate action may stop the current turn and send a distinct new message. Parallel mode is allowed only when each independent interaction can be identified and rendered under its originating message.

Run grouping is purely visual continuity: it must not concatenate user content, assistant content, or usage across messages. An automatic background answer is a separate assistant response with its own lifecycle marker and a link to the task/tool that caused it; it must not reopen a completed user message's answer bubble.

### 4. Provider mapping from available evidence

| Provider | Evidence-based default in this UX | Adapter-specific work |
|---|---|---|
| Claude Code | AgentDeck queue for ordinary follow-ups; explicit interrupt for send-now. | Keep `result` as turn end; tail unsolicited background completion as an auto-turn; preserve `parent_tool_use_id` and task links. Native replay is not the default because it can merge into an existing answer. |
| Codex | AgentDeck queue for ordinary follow-ups; explicit interrupt for send-now. | Filter by parent `threadId`; do not treat child `turn/completed` as root completion. Native `turn/steer` merges, and a second `turn/start` can lose the first prompt; use native queue only for verified eligible persistent threads. |
| Antigravity | Queue next interaction; stop by kill and close locally. | `result` ends the turn; no automatic background answer observed. Child details require an explicit child transcript fetch; parent links may be unavailable. |
| GitHub Copilot | Prefer native enqueue or AgentDeck queue; no implicit immediate steering. | Wait for `session.idle` for run completion; abort then drain terminal events; filter sub-agent events by `agentId`/parent tool; map shell completion auto-turns. |
| OpenCode | AgentDeck queue; do not overlap processes against one session. | Deduplicate repeated parts, treat process exit as completion, and link subagent session IDs to parent calls. |
| OpenRouter/custom chat | AgentDeck-owned queue and turn loop. | No native mid-turn delivery, interrupt, child, or background behavior is assumed. |
| ACP / other connectors | Conservative app-managed queue until probed. | Implement connector-specific mapping and opt into shared helpers only after its behavior is measured. |

This table reflects available probes, not permanent guarantees. Provider versions and modes can change; the strategy must be able to report unknown/unsupported behavior and fall back safely.

### 5. Failure and recovery behavior

- A provider error, process exit, explicit abort, or missing end marker closes only the matching active interaction, once.
- Late events from a stopped/older interaction are ignored for active-turn state but may be retained as scoped diagnostic events if safe.
- If a provider cannot distinguish a child event, it must not append the uncertain text to the root response; show a task/tool result or diagnostic instead.
- If connector state is unknown or a capability is unavailable, use the AgentDeck queue and its own interrupt fallback, and expose the limitation in logs rather than silently switching to parallel/steer.
- On resume, rebuild outstanding queue and interaction associations from persisted message/event IDs; do not infer them from adjacency alone where stable IDs exist.

### 6. Rollout and verification

1. Replace the unused boolean capability surface with the coordinator-facing strategy and wire it into message dispatch, queue, stop, and completion handling.
2. Bring the separate Copilot runtime under the same contract boundary without forcing it into the generic CLI process runner.
3. Migrate each provider one at a time, starting with the already-probed paths; add provider-specific fixtures/conformance cases for message delivery, end, abort, child linkage, and auto-turn behavior.
4. Audit and document unknown behaviors for ACP, Zed, and any other installed connector before enabling native delivery modes.
5. Verify queue ordering and one-answer-per-message through UI-level scenarios, then run the module DoD commands in `adding-agent-provider.md`. Authenticated provider runtime behavior must be reported separately from fixture/build results.

No rollout may enable native steer or parallel delivery solely because a provider supports it. It must first prove that the resulting assistant response remains associated with the correct user message in AgentDeck.

## Out of scope

- Rewriting provider prompts, system prompts, or model quality settings.
- Building a general-purpose plugin marketplace or requiring all integrations to share one transport.
- Reconstructing arbitrary provider history into editable AgentDeck messages; history import remains read-only.
- Automatic semantic classification of whether a follow-up is “related” or “independent.” Queue order and explicit user controls remain deterministic until a separately designed feature is approved.

## Review checklist

- Does this preserve user-visible per-message replies while keeping a continuous run visually grouped?
- Are provider-specific delivery and stop semantics allowed to differ without bypassing the shared transcript invariants?
- Is the conservative fallback acceptable for providers/connectors whose behavior is unmeasured?
- Should Zed-specific history/connector probing be part of the first implementation plan, or remain a separate follow-up?
