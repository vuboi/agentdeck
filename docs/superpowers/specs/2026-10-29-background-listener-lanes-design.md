# Background wait and foreground listener lanes

**Date:** 2026-10-29  
**Status:** Approved in chat; implementation in progress.

## Goal

When an assistant turn is waiting on work that continues in the background, keep that turn alive for its eventual result while allowing a separate foreground interaction to handle new user messages. Preserve message-to-reply ownership across all supported providers and connectors. Normal active generation remains queued according to the current Enter / Alt+Enter / Ctrl+Enter behavior.

## Current evidence

- Rust `agent_runs::RUNNING` and frontend `replies`, `turns`, and `liveReplies` are keyed by chat session, so a chat can have only one live provider run.
- Codex native child work is represented by `collabAgentToolCall` and scoped child `Task` events. `wait` is another tool call that holds the parent CLI turn open.
- `turn/steer` merges input into an active Codex turn; it cannot safely provide an independent answer bubble. Existing probes also show the same risk for Claude replay and Copilot immediate steering.
- The UI already preserves a stable `messageId`/`replyTo` association. The lane work should extend that identity rather than weaken it.
- `blocks.ts` emits an `agent_activity` segment for native task calls. The generic command summary can still count those launch calls, while `AgentActivityCards` counts tasks and repeats per-task tool counts even when detailed child logs are present.
- The ordinary chat path previously omitted `LANGUAGE_RULE`; the current working change now supplies it for every normal prompt.

## Design

### Run lanes

- Give each provider interaction a stable `runId` distinct from the visible chat `sessionId` and the user's `messageId`.
- A chat may retain one suspended/background waiter and one foreground listener. Each run owns its provider conversation ID, reply object, stop handle, status, and message link.
- Only an explicit normalized provider wait/background signal releases the composer. Merely running tools, streaming text, or an unknown status does not.
- A prompt submitted while the background waiter is the only active run creates a fresh provider conversation seeded from a bounded transcript handoff. It receives its own user message and assistant reply. It must not steer the waiting provider turn.
- When no foreground listener is active, later messages during the same wait can start another foreground run from the latest transcript context. If the listener itself is actively generating, existing queue controls apply.
- Completion and stop are keyed by `runId`; one run cannot close or overwrite another. The background parent continues to receive its original child results and completes under its original user message.

### Provider and connector behavior

- Add an explicit `background_wait` lifecycle event to the response adapter contract, with provider-specific evidence and task/tool correlation.
- A provider strategy creates an independent conversation for a listener run. Providers with a native fork may use it; otherwise start a separate conversation/session with a bounded handoff of the visible transcript. Do not use merged steering as the listener lane.
- Keep connector-specific event parsing, session creation, stop behavior, and terminal acknowledgement in its adapter. Unknown connectors remain conservative and do not release the composer from guessed text.
- Persist lane/provider-session identity on the affected transcript reply so reload and late events can be routed correctly.

### Workspace and failures

- Parallel lanes use the same project context and working directory unless the provider explicitly offers an isolated worktree. Include active task descriptions in the handoff so agents can avoid overlapping ownership.
- If starting a separate provider session fails, retain the user's message and report the failure under that message; never silently steer the waiter or lose the prompt.
- Stopping one lane must not stop another. A chat-wide stop remains an explicit operation over every run.
- Late events are accepted only for their matching run and message; stale completion cannot release another lane's queue or replace its live reply.

### UI summary

- Keep one `Agent activity` section for native children/background tasks.
- If a command group consists only of task-launch calls, replace the generic `Ran N commands/tools` line with an agent-task summary; do not show both summaries for the same calls.
- In each expanded task, show the tool count only when detailed child logs are unavailable. When logs are visible, the rows themselves provide that information.
- Preserve command counts and expandable output for ordinary tools.

## Acceptance criteria

1. A Codex parent waiting on child agents remains alive while a separate prompt starts and returns under its own user message.
2. When the child results arrive, the original run finishes under its original prompt; neither reply overwrites, merges into, or closes the other.
3. Same semantics are available through Claude, Antigravity, OpenCode, Copilot, custom/OpenRouter, and registered connectors using independent sessions; unsupported provider behavior stays explicit.
4. Enter/Alt+Enter queue behavior remains unchanged while the foreground lane is actively running; Ctrl+Enter still targets the foreground lane.
5. Stop one run leaves the other run and its child work intact; explicit chat-wide stop handles both.
6. A running prompt in Vietnamese keeps all generated progress and response text in Vietnamese even when the attached file is English.
7. Agent spawn calls are summarized once; per-agent tool counts are not repeated when detailed logs are shown.

## Risks

- Some upstream CLIs cannot fork an existing conversation. Their listener will be a separate conversation seeded with a bounded summary; it may not inherit hidden provider-only state.
- Both lanes may touch the same project files. The provider should receive the other lane's task/ownership context; strict isolation requires worktrees and is not implied where the provider cannot supply one.
- Provider adapters differ in how they signal a true wait. Do not infer it from generic words such as “waiting” in assistant text.
- The current workspace has unrelated uncommitted changes. Implementation must preserve them and must not stage or commit them.
