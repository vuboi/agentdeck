# Claude Code 2.1.306 headless stream-json: live behaviour

Runs: 6 haiku runs (e1, e2a, e2b, e3, e4, e5). Raw stdout: `live-claude-<exp>.jsonl`. Stdin log: `live-claude-<exp>.sent.txt`. Runner: `live-runner.mjs`.
Flags: `-p --input-format stream-json --output-format stream-json --verbose --replay-user-messages --model haiku --permission-mode bypassPermissions` (+ `--include-partial-messages` in e2a only).

## Timelines

**e1: 3 messages during `sleep 15` Bash** (offsets measured from the tool_use event at 3.9s, not wall clock)
- user (replay, initial) -> assistant tool_use(Bash) -> rate_limit_event
- +3s/+6s/+9s: stdin sent; nothing emitted while the tool runs
- system/task_started (tool_use_id = Bash tool_use id, is_backgrounded, task_type)
- system/task_notification (status completed, tool_use_id) -> user tool_result
- user replay x3 (ALPHA, BRAVO, CHARLIE), consecutive, right after the tool_result
- thinking_tokens, assistant thinking, assistant text ("DONE ALPHA BRAVO CHARLIE")
- result/success (one result, turns=2)
- Result: queued messages are merged into the same turn; the answer covers all three.

**e2a: message sent while text streams** (partial messages on)
- stream_event text_delta -> send "ZULU" -> the first answer still completes ("1 ... 60 END")
- result #1 (turns=1)
- system/init + system/status("requesting") again, user replay (ZULU), assistant text "END ZULU", result #2
- Result: a message sent during text generation becomes a second turn, with its own result. A new system/init marks the new turn.

**e2b: message sent right after result #1**
- result #1 ("ONE") -> send "TWO" -> user replay -> assistant "TWO" -> result #2
- Result: same as e2a; the turn boundary is the result.

**e3: Agent (sub-agent) tool**
- assistant tool_use(Agent) -> system/background_tasks_changed, system/task_started (task_id, tool_use_id = Agent id, subagent_type, spawn_depth, prompt, is_backgrounded)
- user tool_result(Agent) "still running in background" -> assistant text -> **result #1 ends the turn early**
- Sub-agent events (all carry `parent_tool_use_id` = Agent tool_use id, `isSidechain` absent): assistant tool_use(Bash), system/task_progress (usage, last_tool_name), user tool_result, assistant text
- system/task_updated, system/task_notification (status completed, summary, output_file, usage), background_tasks_changed
- A new top-level turn starts by itself (parent=null assistant text "Directory empty", no user message) -> **result #2**
- The process kept running after stdin was closed until the sub-agent finished.

**e4: `sleep 20 && echo bg` with run_in_background**
- tool_use(Bash) -> system/background_tasks_changed, system/task_started (is_backgrounded) -> user tool_result "Command running in background" -> assistant "STARTED" -> **result #1 (12.6s)**
- Idle for 20s; at 31s: system/task_updated, system/task_notification (completed, tool_use_id, output_file), background_tasks_changed
- A new turn starts without user input: assistant "Background job ... finished" -> **result #2 (33.2s)**
- Process stayed alive until stdin closed (hold timer); exit code 0.

**e5: interrupt mid-tool** (control_request, subtype interrupt, request_id "stop-1", at +3s into the tool)
- control_response: subtype success, `response.still_queued: []`
- user tool_result: "The user doesn't want to proceed..."
- user text: "[Request interrupted by user for tool use]"
- **result/error_during_execution**: is_error=true, terminal_reason=aborted_tools, stop_reason=tool_use, result=""
- Follow-up sent on the result: replay -> assistant "AFTER" -> result/success (is_error=false)
- The session survives the interrupt. The conversation goes on.

## Fields that link events
- `session_id`: same for the whole process, including all turns and sub-agents.
- `uuid`: unique per event. Replayed user messages get their own uuid, not the sent one.
- `message.id`: one API message, shared by its thinking and text assistant events (e1 Dgh3dPQW).
- `parent_tool_use_id`: null at top level; on sub-agent events = the Agent tool_use id.
- `isSidechain`: not present in stream-json output (use parent_tool_use_id).
- `tool_use_id`: links tool_use <-> tool_result <-> system/task_started/task_notification.
- `task_id`: links task_started / task_progress / task_updated / task_notification.
- `isReplay` on user events: true for echoed user input.
- `result`: one per turn; `num_turns` and `duration_ms` are per turn.

## Rules for the UI
1. A turn ends at `result`, not at process end. Group assistant/tool events by turn between consecutive results.
2. A message sent during a tool is merged into the turn that is running. It appears as user replay after the tool_result. Messages sent while text is streaming, or after a result, start a new turn (new system/init, own result).
3. Sub-agent and background output: attribute events by `parent_tool_use_id` (sub-agent) or `task_id`/`tool_use_id` (background). Show them under the parent tool card.
4. The Agent tool and run_in_background return early: result #1 can arrive while the work is still running. Keep the session open. Background completion (task_notification) starts a new top-level turn with no user input; render it as a new answer in the same chat.
5. The UI should track open tasks: a task is done at task_notification status completed.
6. Interrupt: send control_request interrupt; wait for the error_during_execution result (aborted_tools), not the control_response. Then mark the turn as stopped (not failed), as AgentDeck's `stopped()` does. A follow-up message works on the same process.
7. Partial text (text_delta) appears only with --include-partial-messages. Without it, assistant text arrives as whole blocks.

## Runner notes
- e1 did not close stdin (a runner bug, now fixed); the process was killed at the 120 s timeout after its final result. No events came after that result.
- The e2a flag `--include-partial-messages` was added only for that run.
