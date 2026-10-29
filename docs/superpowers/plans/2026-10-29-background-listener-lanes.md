# Background wait and foreground listener lanes — implementation plan

## Goal and constraints

Implement the approved split-lane design while preserving the existing queue shortcuts for ordinary active generation. Work in the existing dirty checkout; preserve unrelated changes and do not stage or commit.

## Delivered in this slice

1. Reuse normalized provider evidence: `Task` lifecycle events and `ToolCall.background`. No assistant-text heuristics. Providers that do not emit those events remain on the existing queue behavior.
2. Key active Rust runs by visible chat plus turn ID. A second run does not replace the first; stop can target a turn or all turns. Event envelopes retain run and message identity. Copilot follows the same run registry.
3. Track frontend replies by turn ID. Enter during explicit background work starts a fresh provider conversation with bounded transcript context and a separate reply-to link. Alt+Enter remains queued; Ctrl+Enter interrupts. A listener run does not replace the background provider session ID or reply.
4. Listener errors remain attached to the listener's typed message. Stopping a listener leaves its background parent running; explicit interrupt stops both lanes.
5. Remove repeated activity counts: hide Codex `wait N agent(s)` when Agent activity already shows the child tasks, and hide per-agent tool count when detailed child command rows are present.
6. Add focused tests for routing and duplicate wait summaries.

## Follow-up / limits

- Listener provider IDs and lane state are ephemeral; refresh/resume persistence is not implemented in this slice.
- Real authenticated Codex wait flow has not been exercised in AgentDeck Dev. Provider adapter fixtures and build checks provide the current evidence.
- Both lanes share the project's working directory. The listener prompt warns against editing files likely owned by the background task, but this is not enforced with file locks or isolated worktrees.
- Providers without explicit task/background events stay queued until an adapter exposes reliable lifecycle evidence.

## Validation performed

- `pnpm --filter desktop check`
- `node --experimental-strip-types --test apps/desktop/src/lib/run-lanes.test.ts`
- `cargo check -p agentdeck-desktop`
- `cargo clippy -p agentdeck-desktop -- -D warnings`
- `cargo test -p ag-adapters -- --test-threads=1`
- `git diff --check`
