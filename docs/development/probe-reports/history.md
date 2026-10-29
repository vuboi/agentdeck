# Local chat history stores, Windows user C:\Users\boi.vn

Read-only survey. No CLI was run and nothing outside this probe folder was written. All text, paths and ids in examples are replaced with <text>, <path>, <id>, <uuid>, <ts>.

Summary table

| Provider | Store | Format | Transcript present | Sessions |
|---|---|---|---|---|
| Claude Code | .claude\projects\<cwd-slug>\<uuid>.jsonl (+ <uuid>\subagents\agent-<id>.jsonl) | JSONL | yes | 203 top-level, 112 sub-agent files |
| OpenAI Codex | .codex\sessions\YYYY\MM\DD\rollout-<ts>-<uuid>.jsonl (400 are .jsonl.zst) | JSONL (some zstd-compressed) | yes | 609 |
| OpenCode | .local\share\opencode\opencode.db (+ -wal) | SQLite, JSON payloads | yes | 143 (session_v2) |
| Antigravity agy | .gemini\<variant>\brain\<convId>\.system_generated\logs\transcript.jsonl; .gemini\<variant>\conversations\<convId>.db | JSONL + SQLite with protobuf payloads | yes | 99 transcript folders, 113 DBs |
| GitHub Copilot CLI | .copilot\session-state\<uuid>\events.jsonl | JSONL | yes | 13 with events (16 folders) |

---

## 1. Claude Code

### 1.1 Layout
- ~/.claude/projects/<cwd-slug>/<sessionUUID>.jsonl: one file per session (203 top-level). 13 project folders.
- ~/.claude/projects/<cwd-slug>/<sessionUUID>/subagents/agent-<agentId>.jsonl: one file per sub-agent run (112 files). File name matches the agentId field in every record.
- ~/.claude/projects/<cwd-slug>/<sessionUUID>/custom-title.json: title sidecar (43 + 10 folders).
- ~/.claude/history.jsonl: prompt history only (92 lines, keys display, pastedContents, project, sessionId, timestamp). Not a transcript.
- ~/.claude/sessions/<pid>.json: live process metadata. Not a transcript.
- ~/.claude/todos does not exist.
- Totals: 315 JSONL files, 354,444 lines, 0 malformed.

### 1.2 Envelope (most records)
`{type, uuid, parentUuid, logicalParentUuid?, sessionId, timestamp, version, cwd, gitBranch, entrypoint, userType, isSidechain, agentId?, promptId?, slug?}`
- entrypoint seen: cli, claude-desktop, sdk-cli, sdk-ts.
- parentUuid forms a tree. A new turn starts with parentUuid null. A compact_boundary starts a new segment with logicalParentUuid.

### 1.3 Top-level record types (count)
- attachment 123,535 (see 1.7)
- assistant 59,619
- user 39,367
- api-request-blob 30,701, api-request 20,719, api-request-shape 346: request debugging dumps, very large. Skip them when parsing.
- queue-operation 7,852
- system 2,570
- file-history-snapshot 2,154, file-history-delta 1,638
- custom-title 13,530, agent-name 12,414, ai-title 906, last-prompt 14,797, mode 6,704, atis-latch 14,421, permission-mode 37, relocated 2,945, isolation-latch 1
- cost-state 188 (per-session totals, modelUsage map)

### 1.4 user records
- message.content: string, or blocks of type text, image (base64), tool_result {tool_use_id, content, is_error}.
- origin.kind: human (2,229), peer (76), task-notification (245).
- promptSource: sdk (2,473), system (295), typed (21).
- turnOrigin: human (2,106), sdk (240), peer (76), task_notification (67).
- toolUseResult: object shape depends on the tool (see 1.8). Also present on tool result records.
- sourceToolAssistantUUID / sourceToolUseID: link a tool result to its assistant record.
- queueSkipSlashCommands (73), queueSkipAttachments, queueTranscriptOnly, mergedSendUuids (2): set on user records that were queued.
- isMeta (1,202): hidden records (skill/command bodies).
- isCompactSummary (18): summary written after compaction.

### 1.5 assistant records and grouping
- One record per content block. The same message.id and requestId repeat across the blocks of one API response (for example thinking, then text, then tool_use). This happened in 20,760 id groups, and every group had one requestId.
- stop_reason on the last block is end_turn or tool_use.
- message.usage (input_tokens, output_tokens, cache_read_input_tokens, cache_creation_input_tokens, service_tier, iterations) on every assistant record.
- Errors: error (string), isApiErrorMessage, apiErrorStatus (55 records). Model id "<synthetic>" marks client-made error messages.
- Skeleton: `{type:"assistant", uuid, parentUuid, isSidechain, message:{id, model, role, content:[{type:"tool_use", id:<id>, name:"Agent", input:{...}}], stop_reason, usage:{...}}, requestId, sessionId}`.

### 1.6 Mid-turn user messages (queue)
- queue-operation records: enqueue (3,931), dequeue (2,785), remove (1,138).
- remove reasons: absorbed_mid_turn (1,108): the queued text was merged into the running turn. delivered_to_agent (9). No reason (21).
- enqueue carries content (the queued text). Dequeue has no content.
- Skeleton: `{type:"queue-operation", operation:"enqueue", timestamp, sessionId, content:<text>}`; `{type:"queue-operation", operation:"remove", reason:"absorbed_mid_turn", commandUuid, deliveryId, content}`.
- Companion record: attachment of type queued_command (1,125) with `{prompt, source_uuid, delivery_id, commandMode, timestamp}`.
- Sessions with queue records: 192 of 315 files.
- Interrupt: a user record whose content is a text block like "[Request interrupted by user ...]" (48 records, 22 sessions).

### 1.7 attachment records
- Hook output (hook_success 68,722, hook_additional_context 4,173, hook_non_blocking_error 3, hook_cancelled 2), environment, model, skill_listing, deferred_tools_delta/record, total_tokens_reminder (21,597), queued_command, task_status, plan_mode, and others.
- Safe to ignore for chat text. Use queued_command, task_status and hook_cancelled for state.

### 1.8 Sub-agents (Agent tool)
- Parent side: assistant tool_use name "Agent" (108 calls in 27 sessions). input {description, prompt, subagent_type, model}. No "Task"-named tool_use was found in this data.
- Parent result: user record with tool_result for that tool_use_id, and toolUseResult `{agentId, agentType, status, description, prompt, isAsync, outputFile, canReadOutputFile, resolvedModel, totalTokens, totalToolUseCount, usage, toolStats, totalDurationMs}`. 99 parent results carry agentId.
- Child side: <session>/subagents/agent-<agentId>.jsonl. Every record has isSidechain:true and agentId. Its first user record is the delegated prompt with parentUuid null.
- Link key: agentId (toolUseResult.agentId = file name). Tool-use id is not repeated in the child file.
- Related: TaskStop (tool_use with task_id), TaskOutput/BashOutput (77 calls).

### 1.9 Background tasks (Bash run_in_background)
- Start: assistant tool_use name "Bash" with input.run_in_background true (183 calls, 33 sessions).
- Start result: user tool_result whose toolUseResult has backgroundTaskId (about 105 plus variants); the text result says it is running in background.
- Live state: attachment task_status `{taskId, taskType:"local_bash", status:"running", outputFilePath, shell:{command, kind, toolUseId}}` (10 seen).
- Completion: a user record with origin.kind "task-notification" (245 records in 31 files). The content is XML:
  `<task-notification><task-id><id></task-id><tool-use-id><id></tool-use-id><status><text></status><summary><text></summary><note><text></note>[<output-file><path></output-file>][<result>...][<usage>...]</task-notification>`
  Tags seen: task-id, tool-use-id, status, summary, note, output-file, result, usage, event.
- The notification is delivered as a user turn with turnOrigin task_notification.

### 1.10 Tool pairing
- assistant tool_use.id = user tool_result.tool_use_id. The result record also has sourceToolAssistantUUID pointing at the assistant record.
- tool_result.is_error true: 1,289 records.
- Permission denials: toolDenialKind, permissionDecision {decision, source}.

### 1.11 Turn boundaries, errors, usage
- Turn end: system subtype turn_duration (22; durationMs, messageCount). Per-turn hook report: system stop_hook_summary (2,503).
- Other system subtypes: compact_boundary (18; compactMetadata {trigger, preTokens, postTokens}), informational (7), local_command (6; commandRun), api_error (17; error, retryInMs, retryAttempt, maxRetries).
- Interrupts appear as user text, and tool results with interrupted:true.
- Usage: message.usage per assistant record. Session cost: cost-state (188; totalCostUSD, modelUsage).

### 1.12 Other
- Titles: custom-title, ai-title, agent-name records.
- Resume and branching: sessionId repeats across files for resumed sessions. 208 unique sessionIds across 315 files.

---

## 2. OpenAI Codex CLI

### 2.1 Layout
- ~/.codex/sessions/YYYY/MM/DD/rollout-<ISO-time>-<uuid>.jsonl. One file per session.
- 609 rollouts: 204 plain .jsonl and 400 .jsonl.zst under sessions, plus 5 .zst under archived_sessions. Some files are zstd-compressed, so a plain glob of rollout-*.jsonl misses about two thirds of them.
- Other stores: state_5.sqlite (threads: 600 rows, rollout_path, thread_spawn_edges: 9), codex-dev.db (local_thread_catalog: 457), logs_2.sqlite (logs: about 49k), memories_1.sqlite, goals_1.sqlite, history.jsonl (5 lines, {session_id, ts, text}), .codex-global-state.json (queued follow-ups per thread), .sandbox logs.
- 336,115 lines, 0 malformed.

### 2.2 Envelope and top-level types
Every line: `{timestamp, ordinal, type, payload}`.
- response_item 167,801
- event_msg 139,722
- turn_context 10,248
- token_usage_record 16,631
- world_state 806
- compacted 259
- session_meta 609
- inter_agent_communication_metadata 39

### 2.3 Key payloads
- session_meta: `{id, session_id, timestamp, cwd, originator:"vscode"|"exec", cli_version, source:"vscode"|"exec"|{subagent:{thread_spawn:{parent_thread_id, depth, agent_path, agent_nickname, agent_role}}}|{subagent:{other:"guardian"}}, thread_source:"user"|"subagent"|"guardian_review", model_provider, git:{branch, commit_hash, repository_url}}`
- turn_context: `{turn_id, cwd, approval_policy, sandbox_policy:{type}, model, effort, summary, collaboration_mode}`
- event_msg/task_started: `{turn_id, started_at, model_context_window}`
- event_msg/task_complete: `{turn_id, last_agent_message, started_at, completed_at, duration_ms, time_to_first_token_ms}`
- event_msg/turn_aborted: `{turn_id, root_turn_id, reason:"interrupted", started_at, completed_at, duration_ms}`
- event_msg/item_completed: `{thread_id, turn_id, item:{type:"UserMessage"|"AgentMessage"|"Reasoning"|"CommandExecution"|"FileChange"|"McpToolCall"|"ContextCompaction"|"Extension"|"WebSearch"|"ImageView"|"SubAgentActivity"|"Plan", id}, completed_at_ms}`
- event_msg/token_count: `{info:{total_token_usage:{input_tokens, cached_input_tokens, output_tokens, reasoning_output_tokens, total_tokens}, last_token_usage, model_context_window}, rate_limits}`
- response_item/message: `{id, role:"developer"|"user"|"assistant", content:[{type, text}], internal_chat_message_metadata_passthrough:{turn_id}}`
- response_item/function_call: `{id, name, arguments:<json string>, call_id, turn_id}`
- response_item/function_call_output: `{id, call_id, output:<string>}`
- response_item/custom_tool_call (name "exec" or "js"): `{id, status, call_id, name, input}`; output as custom_tool_call_output with `output:[{type:"input_text", text}]`.
- response_item/agent_message: `{id, author:<path>, recipient:<path>, content}`
- token_usage_record: `{payload:{session_id, thread_id, turn_id, root_turn_id, response_id, usage, turn_token_usage, thread_token_usage}}`
- compacted: `{payload:{compaction_response_id, guardian_history, latest_token_usage_record}}`

### 2.4 Patterns
- (a) Mid-turn user message: a second UserMessage item with the same turn_id inside an open turn (30 files). Queued follow-ups that were not yet sent live in .codex-global-state.json under queued-follow-ups, keyed by thread id: `{id, text, context, cwd, createdAt, pausedReason}`.
- (b) Sub-agents: spawn_agent (9 calls), wait_agent (28), send_message (20), followup_task (3), interrupt_agent (4), list_agents (11). Child session_meta has source.subagent.thread_spawn.parent_thread_id. Guardian reviewers are source.subagent.other "guardian" (35 files). thread_spawn_edges in state_5.sqlite maps parent to child.
- (c) Background or long cells: an exec or js custom tool returns "Script running with cell ID <n>". It is polled with `wait {cell_id, yield_time_ms}` or stopped with `terminate`. Completion comes as "Script completed" (or "Script failed") with Exit code. CommandExecution items also have process_id (174 files).
- (d) Turn grouping: task_started ... task_complete or turn_aborted, bracketing items with the same turn_id. token_usage_record carries response_id and root_turn_id.
- (e) Tool pairing: call_id links function_call or custom_tool_call to its *_output.
- (f) Interrupts: turn_aborted with reason "interrupted" (164 events, 115 files). After an abort, 9 turns restart with task_started. 57 files end with an open turn. Errors have no dedicated type: they appear as "Script error"/"Script failed" outputs (200 files), and as status "failed" on McpToolCall (396) or CommandExecution (1,334). Usage: token_count (585 files) and token_usage_record (256 files). compacted in 104 files.

### 2.5 Counts (609 rollouts)
- Has turns: 608. Sub-agent source: 47 (guardian 35, spawn 12). Parent link: 44. Interrupts: 115. Multi-user-message turn: 30. Background cell: 112. Script completed: 304. Script error: 200. token_count: 585. token_usage_record: 256. Compacted: 104.

---

## 3. OpenCode

### 3.1 Layout
- Live store: ~/.local/share/opencode/opencode.db (about 100 MB, with opencode.db-wal and -shm). SQLite.
- Also: storage/session_diff/*.json (30 files), shell/<hash>/sh_*.out (294 spilled shell outputs), log/opencode.log (about 50k lines), log/<date>.log.
- Not chat data: .config/opencode (config, plugins), .local/state/opencode (service.json, contains a secret, not read), AppData\Roaming\opencode (WebView2 profile), AppData\Local\opencode (binaries). account.json and auth.json hold credentials and were not read.

### 3.2 Tables
- Legacy: session, message, part (message and part data are JSON in a `data` column). 53 sessions, 2,577 messages (2,300 assistant, 277 user), 10,635 parts.
- Current: session_v2 (143 sessions; parent_id, fork_*, time_idle, idle_outcome, resume_attempts), session_message (3,968 rows; id, session_id, type, seq, time_*, data JSON). kv key migration.v1-v2 records the migration.
- Other: event (0), session_inbox (0), permission, todo (83), project (14), instruction_*, credential, account.

### 3.3 Record types
- Legacy part.data types: text, reasoning {text, time}; step-start {snapshot}; step-finish {reason, cost, tokens}; tool {callID, tool, state:{status:"completed"|"error"|"running", input, output, metadata, time, title}}; patch {hash, files}; file {mime, filename, url}; compaction {auto, overflow}.
- Legacy message (assistant): {role, agent, mode, parentID (user message id), modelID, providerID, cost, finish:"tool-calls"|"stop"|"length"|"unknown", error?:{name:"MessageAbortedError"|"UnknownError"|"APIError"}, time, tokens:{input, output, reasoning, cache:{read, write}}}.
- Current session_message.type:
  - user: {text, time, files?}
  - assistant: {agent, model, content:[reasoning | text | {type:"tool", id, name, state}], finish, cost, tokens, time, error?}
  - idle: {outcome:"succeeded"|"failed", error?:{type:"provider.auth"|"provider.free-limit"|"provider.invalid-request"|"provider.no-route"|"provider.transport", status, message}}
  - system, synthetic {source:"shell", shellID, jobID, state:"completed"|"cancelled", exit?}, compaction, model-switched.

### 3.4 Patterns
- (a) Mid-turn user message: no marker. session_inbox is empty. 1 of 421 current user rows was created while an assistant turn was open.
- (b) Sub-agents: child session via parent_id on session rows. Legacy 13 children under 10 parents; current 21 children under 12 parents. Spawn tool calls are named "task" (legacy 14) or "subagent" (current 8). Agents: explore, general, build, code-explorer.
- (c) Background shell: a shell tool call with background:true returns metadata.shellID. Completion is a synthetic row with metadata.source "shell", jobID, state completed or cancelled (8 starts, 8 completions: 4 completed, 4 cancelled).
- (d) Grouping: legacy assistant rows have parentID = user message id (274 distinct parents for 2,300 rows). Current rows have no parent id, so a turn is the run from a user row to the next idle row (166 idle rows).
- (e) Tool pairing: legacy part.callID (unique per part). Current: content item id.
- (f) End and errors: finish on message, or idle outcome. Legacy errors: MessageAbortedError 28, UnknownError 19, APIError 1. Current: 22 assistant finish="error", 26 failed idles (auth 13, no-route 4, free-limit 3, invalid-request 3, transport 3). Tokens and cost on the message row. Totals: cost 9.26.

---

## 4. Antigravity (agy and IDE)

### 4.1 Layout
- Variants under ~/.gemini: antigravity-cli (CLI, 91 conversations), antigravity-ide (IDE, 7), antigravity (1). Browser profile dirs are not chat data.
- Transcripts: <variant>\brain\<convId>\.system_generated\logs\transcript.jsonl (plus transcript_full.jsonl with the same line count, and logs\chunks\transcript\N.jsonl).
- Step databases: <variant>\conversations\<convId>.db (SQLite; 113 files, 14 with no steps).
- Queued and background messages: brain\<convId>\.system_generated\messages\<uuid>.json (223 files). Background task logs: brain\...\tasks\task-N.log (179).
- Summaries: antigravity-cli\conversation_summaries.db (106 rows), antigravity\conversation_summaries.db (53 rows). Protobuf indexes: jetbox_summaries_proto.pb, agyhub_summaries_proto.pb.
- Prompt history: antigravity-cli\history.jsonl (2 lines). Not a transcript.
- Not chat data: antigravity*_state.pbtxt, installation_id, AppData\Local\agy (bin only), AppData\Local\Antigravity (staging only).

### 4.2 Record types
- transcript.jsonl lines: `{type, source, status, step_index, created_at, content?, thinking?, tool_calls?:[{name, args}], input_tokens?, output_tokens?, cache_read_tokens?}`.
- Types: PLANNER_RESPONSE 1,961; GENERIC 928; RUN_COMMAND 486; CODE_ACTION 240; USER_INPUT 231; SYSTEM_MESSAGE 210; ERROR_MESSAGE 15; other tool-result types (VIEW_FILE etc.). About 4,670 lines.
- Status: DONE 4,452, RUNNING 179, ERROR 39.
- messages/<uuid>.json: `{id, sender, recipient, deliveryStrategy, priority, hideFromUser, timestamp, content, sourceMetadata:{tool:{conversationId, stepIndex, toolCall:{id, name, argumentsJson}}}}`.
- Step DB table steps: idx, step_type, status, has_subtrajectory, metadata, step_payload, task_details, render_info, error_details, permissions. The payloads are protobuf, not decoded. Other tables: trajectory_meta, gen_metadata, executor_metadata, parent_references (always empty).

### 4.3 Patterns
- (a) Mid-turn user message: messages/*.json with sender "user", deliveryStrategy WHEN_IDLE (12) or NEXT_INVOCATION (1), priority HIGH. Also a USER_INPUT transcript line.
- (b) Sub-agents: BROWSER_SUBAGENT lines (32) and has_subtrajectory steps (32). No child conversation points back to its parent: parent_conversation_id and nesting_depth are empty in every summary. Child output appears only as task-N messages.
- (c) Background task completion: messages with sender <convId>/task-N (154; 135 HIGH and visible, 19 LOW and hidden), with sourceMetadata.tool linking conversationId, stepIndex and toolCall id. Matching tasks\task-N.log.
- (d) Grouping: no turn or group id. A USER_INPUT opens a turn; the turn closes at the last PLANNER_RESPONSE with no tool_calls (180 turns closed that way, 51 did not).
- (e) Tool pairing: tool_calls have name and args only, no id. Pairing is by adjacency and step_index. Ids exist only in message toolCall.id.
- (f) Errors and usage: ERROR_MESSAGE lines (15; error_code 429 x12). No explicit cancel marker. Tokens on 719 PLANNER_RESPONSE lines.

### 4.4 Counts
- 99 brain transcript folders, 113 conversation DBs, about 4,700 steps, summary step_count sums 1,452 and 1,637.

---

## 5. GitHub Copilot CLI

### 5.1 Layout
- ~/.copilot\session-state\<sessionUUID>\events.jsonl: transcript, one per session (13 with events; 16 folders; 3 folders have only lock or partial files).
- Each folder also has workspace.yaml (metadata: id, cwd, git_root, repository, branch, created_at, updated_at, summary_count), vscode.metadata.json, checkpoints\index.md.
- ~/.copilot\session-store.db: SQLite index only. sessions has 1 row; turns, checkpoints, session_files, session_refs, FTS search_index are all empty.
- logs\process-*.log (50), ide\*.lock, mcp-config.json, vscode.session.metadata.cache.json (13 entries).
- config.json is not JSON; settings.json absent.

### 5.2 Envelope and record types
Each line: `{type, id, parentId, timestamp, data:{...}}`. Sub-agent events also carry agentId.
- session.start: {sessionId, version, producer:"copilot-agent", selectedModel, reasoningEffort, context:{branch, cwd, gitRoot, headCommit, repository}}
- user.message: {messageId, interactionId, turnId, content, transformedContent, delivery:"idle", parentAgentTaskId?, attachments?}
- assistant.turn_start / assistant.turn_end: {turnId, interactionId, parentToolCallId?} / {turnId}
- assistant.message: {messageId, originatingMessageId, turnId, interactionId, model, phase:"commentary"|"final_answer", content, toolRequests:[], apiCallId, reasoningText?}
- tool.execution_start: {toolCallId, toolName, toolTitle, arguments, turnId}
- tool.execution_complete: {toolCallId, turnId, success, result:{content, detailedContent?}, toolTelemetry, shellExecution?}
- permission.requested / permission.completed: {requestId, permissionRequest:{kind, toolCallId}, result:{kind}, decisionSource}
- session.usage_record {usage:{model, inputTokens, outputTokens, cacheReadTokens, cacheWriteTokens, reasoningTokens, cost, apiCallId, finishReason, toolCounts}}; session.usage_checkpoint; session.shutdown (totals); session.resume.
- abort {reason:"user_initiated"}
- subagent.started / subagent.configured / subagent.selected / subagent.completed {toolCallId, agentName, agentType, executionMode:"sync", model}
- system.notification {kind:{type:"shell_completed", shellId, exitCode}, content}
- skill.invoked, session.binary_asset (base64 images).

### 5.3 Patterns
- (a) Mid-turn message: not observed. delivery was always "idle" (33 user.message).
- (b) Sub-agents: one run seen. Events carry agentId. Its assistant.turn_start has parentToolCallId, and subagent.started.toolCallId links to the parent tool call. Main turns 108, sub-agent turns 4.
- (c) Background: one system.notification kind shell_completed (shellId, exitCode).
- (d) Grouping: turn_start to turn_end sharing turnId. 116 assistant.message records.
- (e) Tool pairing: toolCallId on start and complete (12 starts unpaired, all in sessions that ended mid-tool). Permissions pair by requestId.
- (f) Aborts: 14 abort events (4 mid-turn). Errors show as success:false. Usage per API call in session.usage_record (111); totals in session.shutdown (25).
- Counts: 1,582 lines (valid JSON). Tool starts 258 (246 completed). Permission requests 222 (all completed). session.resume 13.
- A log file repeats one WARNING line about 1,566 times. Not read in detail.

---

## 5.1 Zed editor (metadata-only check, 2026-10-29)

- `C:\Users\boi.vn\AppData\Local\Zed\db\0-stable\db.sqlite` has editor/workspace metadata; no obvious conversation transcript table was found.
- `C:\Users\boi.vn\AppData\Local\Zed\threads\threads.db` has 3 thread rows. Each `data_type` is `zstd`, with compressed payloads of 906–6,927 bytes.
- Thread summaries and compressed payloads were not read. Zed response/interrupt/child-task semantics remain unknown from this probe. No decompression package or CLI was available, and none was installed.

---
## 6. Cross-provider differences to handle

- Pairing key: Claude uses tool_use.id with tool_result.tool_use_id. Codex uses call_id. OpenCode uses callID in the part, or item id in session_message. Antigravity has no id in the transcript (adjacency only). Copilot uses toolCallId.
- Turn grouping: Claude groups by message.id and requestId, with promptId and turn_duration. Codex uses turn_id with task_started/task_complete. OpenCode uses user to idle (current) or parentID (legacy). Antigravity uses USER_INPUT to last PLANNER_RESPONSE with no tool calls. Copilot uses turnId.
- Mid-turn messages: Claude has queue-operation and queued_command records. Codex has a second UserMessage in the same turn, and queued follow-ups in global state. Antigravity has messages/*.json with deliveryStrategy. OpenCode and Copilot show no marker.
- Sub-agents: Claude uses agentId plus a separate file. Codex uses parent_thread_id in session_meta and spawn_agent calls. OpenCode uses parent_id on sessions. Copilot uses agentId and parentToolCallId. Antigravity has no parent link.
- Background tasks: Claude uses task-notification user records. Codex uses "Script running" then "Script completed" polling. OpenCode uses synthetic rows. Antigravity uses messages/*.json with sender <conv>/task-N. Copilot uses system.notification.
- Interrupts: Claude uses user text "[Request interrupted by user]". Codex uses turn_aborted. OpenCode uses MessageAbortedError. Copilot uses abort. Antigravity has no marker.
- Compressed or binary stores: Codex has .jsonl.zst (about two thirds of rollouts). Antigravity and OpenCode store step payloads in protobuf or JSON inside SQLite.
- Skip large noise: Claude api-request-blob and attachment records. Codex world_state. Copilot logs.

## 7. Caveats
- Antigravity protobuf payloads were only shape-checked, not decoded. Step type numbers are inferred.
- Counts are snapshots. Codex and Antigravity were still being written during the run.
- The OpenCode probe agent ran `rm -f` on *.mjs in this probe folder while cleaning its own scripts. My scripts are .js and were not affected, but any .mjs files that were here are gone.
- Scratch copies of databases and helper scripts are in this folder (probe\db, probe\snap, *.js). They are outside the repo and can be deleted.
