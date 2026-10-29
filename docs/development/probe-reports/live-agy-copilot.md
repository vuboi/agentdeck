# Antigravity (agy) and Copilot CLI: headless behaviour

## Status: no live turns ran for either CLI

- **agy 1.4.9** is installed but **not signed in**. The plain turn (`gemini-3.8-flash-low`, the cheapest listed model) printed a Google OAuth URL, waited 60 s, and exited 1 with `result.status: ERROR`, "authentication failed or timed out". I did not log in. Raw: `live-agy-1-plain.jsonl`, `.err`. Experiments 2 to 4 not run.
- **Copilot CLI** is not installed (the npm package `@github/copilot` is absent). I did not install it. No `live-copilot-*.jsonl` exists.
- Runs used: 1 of 8 (agy), 0 of 8 (Copilot).

Below is from AgentDeck's adapters, agy fixtures recorded with agy 1.4.5, and Copilot SDK 1.0.28 source. Not verified against the installed versions.

## agy

**1. Plain turn (fixtures).** `init` (conversation_id, model, cwd, tools) → `step_update` user_input DONE → `agent_response` ACTIVE with `text_delta` chunks → `agent_response` DONE with `usage` → `result` (conversation_id, status SUCCESS or ERROR, full `response`, num_turns, usage). `result` is the final marker and also arrives on ERROR. Usage appears on the step DONE and on `result`; output includes thinking tokens.

**2. Mid-turn message.** `--input-format stream-json` reads one NDJSON message per turn. AgentDeck only sends the next message after a `result`. Behaviour when a message arrives mid-turn (merged, queued, rejected, parallel) is untested. Do not send mid-turn until measured.

**3. Interrupt.** No cancel path in the adapter. `--print-timeout` (30 m) ends the turn with an ERROR `result`. A killed process emits no `result`, so process exit must end the turn.

**4. Sub-agent and background.** Not observed. Fixture step types: `user_input`, `system_message`, `agent_response`, `tool` (`tool_name`, `tool_info`). The adapter always sends `background: false`.

**Refusals.** A Bash step that fails with "permission", or ends with null output, is a refusal. `result.denied_actions[]` with `action: "command"` raises a permission request.

**Ids.** `conversation_id` is stable across turns. Tool ids are `step_index`, unique within a turn.

## Copilot (SDK-derived, not run)

AgentDeck uses `github-copilot-sdk` 1.0.28 (`apps/desktop/src-tauri/src/copilot.rs`): streaming session, `send_and_wait(MessageOptions)`, event subscription, and on stop `session.abort()` then `disconnect`.

**1. Turn timeline the app consumes.** `assistant.reasoning_delta` → Thinking; `assistant.message_delta` → Delta; `tool.execution_start` (`toolCallId`, `toolName`, `arguments` as JSON string) → ToolCall; `tool.execution_complete` (`toolCallId`, `result`, `error`) → ToolResult; `assistant.usage` → Usage; `session.error` → Error. The turn ends when `send_and_wait` resolves, which emits `Done`. The SDK also emits `user.message`, `session.idle`, `session.task_complete`, and `subagent.*`; AgentDeck ignores these.

**2. Mid-turn messages.** The SDK has a steering lane and a queue. `queueSendNow` returns `steered: true` when accepted into the running turn, and `false` when no main turn is live. Queued items keep submission order. `in_flight_steering_count` shows how many steering messages were folded into the running turn (each emits `user.message`). AgentDeck does not use these calls, so its behaviour is untested.

**3. Interrupt.** `session.abort()`, then the loop breaks with `finished = false`. No `Done` is emitted. The UI must end the turn locally on stop.

**4. Sub-agents.** Each envelope has `id`, `timestamp`, `parentId`, `ephemeral`, and `agentId` (set for sub-agent events, absent for the root). Some tool payloads carry `parentToolCallId`. Lifecycle: `subagent.started`, `subagent.completed`, `subagent.failed`.

**Risk.** `copilot.rs` appends every `assistant.message_delta` to the answer without checking `agentId`, so sub-agent text would merge into the main answer. Verify live.

## What a UI needs

- **Turn end:** agy `result` (any status), Copilot `Done`, or locally on stop or process exit.
- **agy grouping:** `conversation_id` per session, `step_index` for tool pairing.
- **Copilot grouping:** `toolCallId` for pairing, `agentId` to split sub-agent work, `parentToolCallId` to link to the parent tool.
- **Mid-turn sends:** queue locally and send after the turn ends until measured.
- **Background work:** no marker in either adapter; show as a tool call.

## Next steps

1. Sign in to agy yourself, then rerun experiments 1 to 4 with `gemini-3.8-flash-low`.
2. Install `@github/copilot`, then run the SDK with the cheapest model, checking it has no premium multiplier.
