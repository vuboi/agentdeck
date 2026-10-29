# Codex CLI and OpenCode headless behaviour (live probes, 2026-10-29)

Raw output: `live-codex-1-plain.jsonl` … `live-codex-6-subagent.jsonl` (JSON-RPC in/out with timestamps), `live-opencode-1-plain.jsonl` … `live-opencode-5-subagent.jsonl` (`--format json` stdout), plus `.err` files. Scripts: `codex-tools/`. Work dirs: `work-codex/`, `work-opencode/`.

## Codex CLI (0.163.0, app-server over stdio, model gpt-5.6-luna, effort low)

Account is `planType: free`. The weekly rate limit went 91% to 93% during the six runs, so quota is tight.

**Handshake (as AgentDeck):** `initialize {clientInfo, capabilities:{experimentalApi:true}}`, then `initialized`, `thread/start {cwd, model, approvalPolicy, sandbox, ephemeral}`, then `turn/start {threadId, input:[{type:"text",text,text_elements:[]}]}`. The response is `{turn:{id,status:"inProgress"}}`. Noise to ignore: `mcpServer/startupStatus/updated`, `turn/moderationMetadata`, `remoteControl/status/changed`, `account/*`.

**1. Plain turn (run 1)**, ordered:
- `turn/started`
- `item/started`/`item/completed` `userMessage` (echo of input)
- `item/started`/`item/completed` `reasoning` (empty summaries here)
- `item/started` `commandExecution` → `item/commandExecution/outputDelta`* → `item/completed` (`status` completed or failed, `exitCode`, `aggregatedOutput`)
- `thread/tokenUsage/updated` (`last` and `total`: input, cached, output, reasoning)
- `item/started` `agentMessage` → `item/agentMessage/delta`* → `item/completed` (full `text`)
- `thread/status/changed` (idle), then `turn/completed {turn:{id,status:"completed",durationMs}}`

The model ran five commands for a one-line task (it read user skill files), so cost is driven by the model, not the protocol.

**2. Message during a running `sleep 10`**
- `turn/steer {threadId, expectedTurnId, input}` is accepted. It returns the same `turnId`, and the text is added to the active turn as a `userMessage` item. The final answer followed the steer ("STEERED"), with one `turn/completed`.
- A plain second `turn/start` does not start a new turn. It returns the active turn (same id) and appends the input as a `userMessage`. The model answered "SECOND" and `turn/completed` arrived while the `sleep` was still running. The command's `item/completed` came about 7 s after `turn/completed`. **Items can complete after the turn ends.**
- `thread/queue/add {threadId, clientUserMessageId, input}` queues the message (`thread/queue/changed`, `thread/queue/list` work). It fails on `ephemeral` threads (-32600). On a persistent thread the queued message auto-starts a new turn after the current one completes.

**3. Interrupt mid-command**: `turn/interrupt {threadId, turnId}` returns `{}`. `turn/completed` follows about 0.1 s later with `status:"interrupted"`. **The running `commandExecution` never gets `item/completed`**, so the UI must close open items itself when the turn ends. AgentDeck maps interrupted to a failed turn ("Codex stopped the turn").

**4. Sub-agents (run 6).** Items are `collabAgentToolCall` with `tool` `spawnAgent` or `wait`, `senderThreadId`, `receiverThreadIds`, `prompt`, `model`, and `agentsStates` ({status, message}). The child runs on **its own thread**: `thread/started` with a new threadId, and its `turn/*` and item notifications carry that child threadId. The parent `turn/completed` comes after the child's. A UI must filter notifications by `threadId` and nest the child under the `collabAgentToolCall` via `receiverThreadIds`. My first run ended on the child's `turn/completed` because I did not filter, which is exactly the trap.

**Schema also exposes (not run):** `thread/backgroundTerminals/{list,terminate,clean}`, `item/commandExecution/terminalInteraction`, `subAgentActivity` item kind, `turn/diff/updated`, `thread/queue/*`. Server requests (approvals) are unanswered in headless mode; AgentDeck replies with an error.

**Turn end:** `turn/completed` on the parent threadId with `status` completed, failed, or interrupted. `error` with `willRetry:true` is informational.

## OpenCode (v2.2.0, model opencode/deepseek-v4.1-flash-free, no login needed)

Command: `opencode run --standalone --format json --thinking [--auto] [--session <ses_…>] --model <m>`, prompt on stdin. Output is one JSON object per line: `{type, timestamp, sessionID, part:{id, messageID, type, text|tool|state, time}}`.

**1. Plain turn (run 1)**: one line, `type:"text"`, with `sessionID` and the text. Exit 0. No usage and no end marker.

**Tool turn (run 2)**: `step_start` → `reasoning` → `tool_use` (`tool:"shell"`, `state.status:"completed"`, `input`, `output`), emitted once, with no pending or running state → `step_finish` (`reason:"tool-calls"`, `tokens` input, output, reasoning, cache) → `step_start` → `text` → process exit. The final `step_finish` appeared in run 5 (`reason:"stop"`) but not in run 2, so do not rely on it.

**2. Second message while the first is running (runs 3a and 3b, two processes on the same session)**
- The second process started its own assistant message at once. It was **parallel, not queued or merged**. Its answer ("SECOND") came about 10 s before the first run's `sleep 15` ended.
- **Each process's stdout carries events from the whole session.** The second message's reasoning and text appeared in both streams with identical part ids. A UI must dedupe by `part.id` and group by `messageID`.
- The first turn continued and finished after its tool completed. Its last text talked about the sleep being interrupted.

**3. Sub-agent (run 5)**: `tool_use` with `tool:"subagent"`, `input:{agent:"general", description, prompt}`, `state.status:"completed"`. `metadata.sessionID` is the child session id, and the output contains `<subagent sessionID="…" state="completed">4</subagent>`. Child events are not in the parent stream. The child session did not show in `opencode session list`, and I could not confirm a `parent_id` link in the DB, so use `metadata.sessionID` as the link.

**4. Abort (run 4)**: killing the process tree (`taskkill /T /F`) ended the stream after `reasoning`. **No `tool_use` was emitted for the killed call and there is no end marker.** The next run on the same session still worked and noted the interrupted sleep.

**Turn end:** the process exits (code 0). That is the only reliable end signal. Token counts come from `step_finish`, when present.

## What a UI needs

- **Codex:** key everything by `threadId` and `turnId`. Group items under the turn. Treat `turn/completed` as the end, then close any open items, because they can finish later. Use `turn/steer` (same turn) or `thread/queue/add` (next turn). A plain `turn/start` during a turn merges into it. Show `collabAgentToolCall` as a parent with child threads.
- **OpenCode:** group by `sessionID` and `messageID`, dedupe by `part.id`, since streams carry the whole session. The process exit is the turn end. Mid-turn messages run in parallel, so either queue them in the UI or accept interleaved answers. Show `subagent` tool calls as one item and link to the child with `metadata.sessionID`. On abort, close any running tool call yourself.

## Round 2 (2026-10-29, complex orchestration)

Raw files: `live2-codex-A-orch.jsonl`, `live2-codex-B-bg.jsonl` (a PowerShell `Start-Job`, so no real wait), `live2-codex-B2-fg.jsonl` (foreground `Start-Sleep 15` with a mid-run message), `live2-codex-C-c4.jsonl`. OpenCode: `live2-opencode-A-orch.jsonl`, `live2-opencode-B1-first.jsonl` (solo sleep baseline), `live2-opencode-B-first.jsonl` and `live2-opencode-B-second.jsonl` (mid-run message, same session), `live2-opencode-C-four.jsonl`, `live2-opencode-D-agent.jsonl`. Each has a `.err`. Scripts: `live2-scripts/` (`codex-r2.mjs` is a copy of `codex-probe2.mjs` with cwd and prompts changed; `oc-tl.mjs` prints the OpenCode timeline; `oc-b.sh`, `oc-c.sh`, `oc-d.sh`). Codex used the new empty dir `work-codex-r2` because `work-codex` still holds round 1's `a.txt`. Codex: 4 runs. OpenCode: 6 processes. Test files were removed.

### A. Parallel sub-agents

**Codex (3 children, gpt-5.6-luna, effort low):**
1. `turn/started` on the parent threadId `…a6f4ca`, turn `…169942`.
2. `item/started collabAgentToolCall tool=spawnAgent` ×3, one at a time. At start: `receiverThreadIds:[]`, `model:""`, `reasoningEffort:"medium"`, `prompt`. Each completes at about 7.3 to 7.6 s with `receiverThreadIds:[childThread]`, `model:"gpt-5.6-luna"`, `reasoningEffort:"low"`, and `agentsStates{childThread:{status:"pendingInit"}}`.
3. Child threads start with `turn/started` on their own threadIds. No `thread/started` is emitted for them. Their items (`reasoning`, `commandExecution`) carry the child threadId and interleave with each other and with the parent.
4. `item/started collabAgentToolCall tool=wait` with `receiverThreadIds` = all 3 children. It completed at 15.1 s with **only the first child** (`agentsStates` has `status:"completed", message:"4"`). The parent then issued two more `wait`s (`recv=634a06,5861d3`, then `recv=5861d3`). **A `wait` returns when the first child finishes, not when all do**, so one wait per group is not enough.
5. Child `turn/completed` events arrive at 15.1, 16.6 and 17.9 s, each with its own threadId.
6. Parent `item/started agentMessage`, deltas, `item/completed` text `"4,6,8"`, then the **parent** `turn/completed` at 21.3 s. Match by `params.threadId`, not by the first `turn/completed` seen.

The children ran shell commands that read skill files, despite my "do not read files" wording in the parent prompt. Children do not inherit the parent's prompt constraints.

**OpenCode (3 children, deepseek-v4.1-flash-free):**
1. `step_start` (session `…lIpksifN`, message `EtZjcoCW`), then `text` "I'll launch three subagents in parallel."
2. Three `tool_use tool=subagent` parts in the **same** step. Each has `state.time` start and end, and the three intervals overlap (all started within 26 ms, finished 1.4 to 2.4 s later). Each completes with `metadata.sessionID` = child session and `output` = `<subagent sessionID="…" state="completed">N</subagent>`.
3. `step_finish reason=tool-calls`, then a new step (new messageID `3ecGed67`) with `text` "4,6,8". **No `step_finish` after the final step.** The process exits.
4. Child events are **not** in the parent stream (it holds 7 lines). Child progress is only visible in the child's own session, and this stream does not expose it.

### B. Long command plus mid-run message

**Codex** (`live2-codex-B2-fg.jsonl`): `turn/start` on the active thread returns the **same turnId** (`…5943d9`). The message is merged as a `userMessage` item into the running turn. There is one `turn/completed`, and the model answered only "SECOND", so the FG-DONE answer was dropped. The model also emitted an `agentMessage` preamble ("Running the requested foreground sleep now.") **before** the `commandExecution`, so a text item can come before any tool. In the background case (`Start-Job`), the command completed in 0.5 s, with no late event and no new turn started by itself.

**OpenCode** (`live2-opencode-B-first.jsonl`, `…-second.jsonl`; the second process started 4 s after the first, same `--session`):
- The first stream shows `step_start`, `reasoning`, `tool_use shell` (`sleep 15`, status completed, emitted only when it finished at +15.3 s), then `step_finish tool-calls`.
- During the sleep, the second process ran in parallel and finished in 3.6 s. Its reasoning and text ("SECOND", `messageID WqSttEqR`) appeared **in both streams with identical part ids**.
- After its sleep, the **first** process started a new step (`messageID dK0BRA1g`) and emitted text "SECOND" as its final answer, replacing FG-DONE. It answered the newer user message, not its own. Exit 0 about 12 s after the second process started.
- Nothing starts a new turn after the process exits. Baseline (`B1`, no message): `sleep 15` completed, then FG-DONE.

### C. Four tool calls in a row

**Codex** (`live2-codex-C-c4.jsonl`): items in order, all under one turn:
1. `agentMessage` with `phase:"commentary"` and empty `text` (preamble).
2. `fileChange` (write n.txt): `changes:[{path,kind:{type:"add"},diff}]`, `status`. It went inProgress to completed about 1.7 s apart. **This item type has no `durationMs`.**
3. `commandExecution` (`Get-ChildItem -Name`): `id`, `command`, `cwd`, `processId`, `source:"unifiedExecStartup"`, `commandActions`, `aggregatedOutput`, `exitCode`, `durationMs` (162 ms). Status goes `inProgress` to `completed` or `failed`.
4. `commandExecution` (`Get-Content -Raw n.txt`), then `commandExecution` (`echo done`). **The read is a shell command**, not a separate read item.
5. `turn/diff/updated` after each file or command. Then `agentMessage` "DONE", then `turn/completed`.
Four items share one `turnId`. Labels come from `command` or `changes[].path`. Durations come from `durationMs`, or from item timestamps for `fileChange`.

**OpenCode** (`live2-opencode-C-four.jsonl`): each tool call is its own step: `step_start` (new messageID), then `tool_use` (`tool`, `title`, `state.status`, `state.input`, `state.output`, `state.time{start,end}`), then `step_finish reason=tool-calls`. The four steps are `write` (`input.path`, `input.content`), `shell` (`ls`), `read` (`input.path`, output "1: hello") and `shell` (`echo done`). There is one `text` preamble before the first step and a final text step (`messageID KI9x7Gda`). Duration is `state.time.end - start` (about 0.1 to 0.3 s each). For `ls`, `title` is "shell" and the command is in `input.command`.

### D. Delegated worker labels

**Codex:** the `spawnAgent` item carries `prompt`, `model` (empty until the child exists, then `gpt-5.6-luna`), `reasoningEffort` (`medium` requested, `low` applied), `receiverThreadIds`, and `agentsStates[child] = {status, message}` (`message` is the child's final text, e.g. "4"). The parent thread's `agentNickname` and `agentRole` are `null`, and no child metadata came through. There is no name or role to show, so the UI can label children by model plus index or prompt. Whether a role or nickname can be requested was not tested.

**OpenCode** (`live2-opencode-D-agent.jsonl`): the `tool_use tool=subagent` part has `state.input.agent` (the **type**: `general`, `explore`) and `state.input.description` (used as `title`, a human label), plus `metadata.sessionID` (the child session). **No model appears anywhere in the stream** (grep for model, modelID and providerID finds nothing). The UI can show "explore: Label probe" but not the model. `opencode export` does not exist in this build, so the child's model could not be read.

### Rules for the UI (round 2)

- **Codex grouping:** key by `threadId` and `turnId`. Child items are on child threadIds. Nest them under the `collabAgentToolCall` whose `receiverThreadIds` contains that thread. Track `wait` by receiver: a `wait` can complete with one child, so keep the spawn group open until every receiver is `completed`.
- **Codex end of turn:** take the `turn/completed` whose `params.threadId` is the top-level thread. A child's `turn/completed` is not the end. Close open items when the parent turn ends. A mid-run `turn/start` joins the same turn and produces one answer, so answers can be lost.
- **OpenCode grouping:** group by `sessionID` (parent vs child) and `messageID` (one step per tool call), and dedupe by `part.id` (the shared stream repeats parts from a mid-run second process). Child output is only in `metadata.sessionID` and the `<subagent>` wrapper, not in the parent stream.
- **OpenCode end of turn:** the process exit. The last step has no `step_finish`, so do not wait for one. A tool call is emitted only when it completes, so show it as "running" from process start until then.
- **Mid-run messages:** two OpenCode processes on one session run in parallel, and each answers the newer message too. Queue in the UI if you want one answer per message.
