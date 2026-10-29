# Antigravity (agy) and Copilot: live behaviour

Raw logs: `%TEMP%\probe\live3-agy-*.jsonl` (agy 1.4.10, model `gemini-3.8-flash-low`, warm mode `--input-format stream-json -p= --output-format stream-json --dangerously-skip-permissions`) and `%TEMP%\probe\live-copilot-*.jsonl` (Copilot CLI core 1.0.105 reached through the SDK's `session.send` / `abort` / `delivery`, model `gpt-5.6-luna`). Event dumps use the harness format `{ms, harness|src, raw}`. Sign-in was already valid; nothing was signed in or switched during this probe. (An earlier attempt, when agy was signed out, is superseded.)

## Antigravity (agy)

| Question | Observed |
| --- | --- |
| Turn end | `{"event":"result"}` (`status: SUCCESS`, `num_turns` counts turns of the process so far). One result per turn. |
| Message sent mid-turn (during a 10 s `run_command`, +6 s) | **Queued, not injected.** Turn 1 finished with its own result (`DONE1`, 24.6 s); only then a `user_input` step (index 6) appeared and a second turn answered `TWO` (result 2, `num_turns: 2`). Nothing of it reached turn 1. |
| stdin EOF during a running tool | Turn is NOT cancelled: tool finishes, answer and `result` arrive, then the process exits 0. EOF = "finish and quit". |
| Kill (SIGKILL mid-tool) | No end marker at all (last line is the tool step `ACTIVE`). The app must close the turn itself. |
| Interrupt | `agy --help` has no stream-json interrupt message; stop = kill. |
| Sub-agent | Tool step `invoke_subagent` (`ACTIVE`), then a step with `step_type: "subagent"` and `subagent_info.subagents[]` = `{role, initial_prompt, conversation_id, log_uri, workspace_uris}` (the child conversation and its `transcript.jsonl`). The child's own steps and text are **not in the parent stream**; the parent model read the child's transcript with `read_file`. No sub-agent text leaks into the answer. Link = `conversation_id` of the child. |
| Background command | `run_command` stayed `ACTIVE` while the model called `manage_task` (`Action: status`, `TaskId: <conv>/task-N`, output `Status: RUNNING`, `Log: .../tasks/task-N.log`). The turn ended with `result` while the command still ran. **No completion event and no automatic new turn** afterwards (unlike Claude); the process stayed up until stdin EOF. |
| Other | A `system_message` step (no payload) follows tool steps; `agent_response` steps stream `text_delta` and end with `text_delta: "\n"` plus `usage`. |

## Copilot (SDK session events)

| Question | Observed |
| --- | --- |
| Turn end | `assistant.turn_end` ends one model round (a tool call makes 2: `turn=0`, `turn=1`); `assistant.idle` then `session.idle` end the whole run. **`assistant.idle` is not the end while a shell is detached**: with a background shell it fired at 10.8 s and `session.idle` only at 22.5 s. |
| Mid-turn `delivery: "enqueue"` | Listed in `pending_messages` (`items`), answered as a **separate interaction** after the first one went idle: `FIRST-DONE` then (new `ia`) `SECOND-MSG`. |
| Mid-turn `delivery: "immediate"` (steer) | `steeringMessages` set; merged into the same interaction at the next model call: one answer `FIRST-DONE SECOND-MSG`. The synchronous shell was detached by the steer (`tool.execution_complete` at 8 s while `sleep 10` ran on). |
| Background shell completion | After the detach, `system.notification` (`kind.type: shell_completed`, `shellId`, `exitCode`) at 19.4 s started an **automatic new turn** (`read_powershell` then the answer again), like Claude's `task_notification`. `session.background_tasks_changed` fires around it (data empty in the dump). |
| Interrupt (`session.abort()`) | `abort{reason:user_initiated}`, `agent.interrupted`, `model.turn_failed`, then `assistant.turn_end`, `assistant.idle`, `session.idle`. **A normal end sequence follows; no `Done`-like error event**: idle after abort is the marker (works pre-first-token and mid-command). Session reusable. |
| Sub-agent (`task` tool, sync) | `tool.execution_start tool=task` then `subagent.started` (`toolCallId`, `agentName`, `agentDisplayName`, `model`, `executionMode`), child events carry **`agentId`** and `assistant.turn_start` has `parentToolCallId` = the `task` toolCallId, `subagent.completed` (`totalToolCalls`, `totalTokens`, `durationMs`). The child produced only a whole `assistant.message` (`SUBDONE`) with `agentId`; the 3 `assistant.message_delta` and all reasoning/streaming deltas were from the main agent. |

## What this means for AgentDeck

- **agy**: `result` is a reliable turn end; mid-turn text must be queued by the app (the CLI queues it as the next turn anyway, so "inject" is not available). Child agents must be fetched from `log_uri` if shown; background tasks need polling of `manage_task` or the log, no event comes.
- **Copilot** (`apps/desktop/src-tauri/src/copilot.rs:214-294`): the turn is closed from `send_and_wait`'s return and every `assistant.message_delta` is appended to `answer` without an `agentId` check. In this probe sub-agent text did not stream as deltas, but the final `event.data["content"]` taken from the returned message could be a sub-agent's (`assistant.message` carries `agentId`); filter on `agentId` / `parentToolCallId`. The probe used plain `send`, so `send_and_wait`'s return point with tools, steering and background shells is **not measured**. A background shell makes `assistant.idle` early, then an auto-turn: wait for `session.idle` (plus `background_tasks_changed`), not the first idle. Interrupt: after `abort()` read until `session.idle` instead of dropping the stream.

## Code-derived notes (not re-verified)

- agy `finish()` in `antigravity.rs` turns `result` into `Done`; `warm_message` writes `{"event":"user",...}` lines.
- Copilot `steering` / queue (`queueSendNow` returns `steered`) exists in the SDK; the probe confirms `enqueue` vs `immediate`.
