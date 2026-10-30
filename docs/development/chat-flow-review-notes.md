# Chat flow review notes

Status: working tree, not committed. Many files are uncommitted, including work by other people. Review the whole diff, not only the files listed here.

Verification: the last runs were by the agents that made each change. Re-run before merging:

- `cargo clippy --workspace -- -D warnings`
- `cargo test -p ag-adapters`
- `cargo test -p agentdeck-desktop`
- `pnpm exec svelte-check` in `apps/desktop`

The OpenCode, Antigravity and Copilot changes have not been run against a live provider. Their event shapes come from captured fixtures or the installed binary's schema.

## 1. Frontend chat flow (`apps/desktop/src/lib/chat/`)

Hangs and dead threads:

- `session/events.svelte.ts`: an exception in one event handler no longer drops the rest of that chat's queue or its `done`. `applyEvent` logs and continues, and the flush timer is re-armed if work remains.
- `session/events.svelte.ts`: after a failed account switch, or a switch that finished while the user was replying, the queue is drained (`sendNextQueued`).
- `session/events.svelte.ts`: `runRecovery` does not reset the conversation if the user has already sent a message.
- `session/events.svelte.ts`: "Try again" does nothing while a reply is running. The resume card stays.
- `session/turns.svelte.ts`: `stopTurn` clears a stuck primary lane when no reply is open. Previously Stop did nothing and the queue stayed blocked.
- `session/turns.svelte.ts`: stopping a lead chat stops its running workers through their tasks (`stopDelegateTask`), so no unwanted hand-back is delivered.
- `session/turns.svelte.ts`: `closeRunningActivity` runs only for a real Stop, not for a quiet interrupt. It also closes the audit entries of the requests it drops.
- `session/delegate-task.svelte.ts`: a worker chat is kept in `localMessages`, so a history refresh cannot delete it while it waits for a slot.
- `session/delegate-task.svelte.ts`: a worker inherits its caller's permission and effort (`chatTurnOptions`).
- `session/delegate-task.svelte.ts`: denying an approval on a worker ends its task. Allowing it sets the task back to running (`answerWorkerApproval`).
- `session/delegation.svelte.ts`: `settleWorkflowRun` ends a flow run when its workers were stopped and no report is pending.
- `session/sending.svelte.ts`: a silent send that is dropped because a reply is running calls `onDropped`. The report goes back into the backlog.
- `session/history.svelte.ts`: `deleteSession` stops the chat's turn and clears its per-session state (queue, lanes, replies, recoveries, resumes, transcript cache, backlog, last event times).
- `session/run.svelte.ts`: custom-provider start and picture reads have timeouts (`within`). A start timeout also stops the turn.
- `session/run.svelte.ts`: starting a turn clears `manuallyStopped`.
- `session/run.svelte.ts`: `updateLane` skips unchanged lanes, to cut per-delta work.
- `runs/usage.ts`: a failed usage write no longer rejects the chain, so later records are still written.
- `session/turns.svelte.ts`: Stop now also cancels workers in `queued/idle`, not only workers already `running`; otherwise their slot waiter could start after the lead chat was stopped.

Per-chat settings:

- `session/turns.svelte.ts`: `chatTurnOptions` and `turnOptionsOf(sessionId)`. `Composer.svelte` writes them per chat. Background turns read their own chat's permission and effort, not the on-screen chat's.

Not done (frontend):

- Keep a deferred send (`waitForCredential`, `afterSwitch`) as a visible, cancellable queue entry.
- Clear the flow map entry (`activeWorkflowByMain`) in `finishWorkflowIntegration` when the run ends.
- Per-token or per-delta work in `handleEventNow` (`advanceRun`, `askText`, project lookups) is still heavier than needed. `runStates` looks unused outside `state.svelte.ts`.
- `persistLocal()` is called without a session id everywhere, so every save re-serialises all local chats. `dirtyChats` is effectively dead.
- `work.svelte.ts` re-projects all background work on every output delta.
- `callerOf` can pick the wrong chat when several are running and the link does not name one.
- `runStates`/`run-state.ts` is not currently the source of truth for the UI: composer and delivery use `sessionLanes`, `replies` and `status`. Its tests are useful reducer tests, but do not prove the actual multi-lane flow.

## 2. Rust backend (`apps/desktop/src-tauri/src/`)

- `custom_chat.rs`: Bash tool reads stdout and stderr concurrently with a 256 KB cap each. A process tree is killed on timeout or drop. The model call has a 300 s header timeout. Bing title and snippet whitespace is collapsed.
- `opencode_serve.rs`: evict only when the connection broke or the turn is the only holder. `Held` guard releases the busy counter. The server starts outside the lock, behind a single-flight `STARTING` mutex. Stall watchdog (10 min). Permission requests are answered through the same verdict path as Claude. Done carries the final text and summed cost. A stop always ends with `Done`.
- `copilot.rs`: the client starts outside the `POOL` lock, under a 90 s timeout. `ClientHold` guard releases the busy counter. Lagged events are ignored, a closed channel is an error, and a stall watchdog was added. Debug code (`// DBG`) removed. Refused requests in plan mode emit `PermissionDenied` with allow entries. Sub-agent progress carries a tool count. A stop ends with `Done`.
- `agent_runs.rs`: a forget before track leaves a tombstone. Poisoned locks (`RUNNING`, `PENDING`) recover their inner value instead of silently skipping.
- `delegate.rs`: the worker gate is per owner chat. Poisoned `GATE` recovers its value and stays fail-open. Delegate events use `emit_to("main", ...)`. The `delegate_task` description has the guidance text the test expects.
- `adapters/src/agent/codex_server.rs`: a notification for a different turn id is ignored. A parked server is drained before reuse.
- `adapters/src/agent/tail.rs`, `warm.rs`: `tail::discard` on account switch and on stop-all. At most 8 tails. Non-Codex prewarm takes the chat lock and skips if a tail holds the chat.
- `adapters/src/agent/antigravity.rs`: refusals carry the tool-call id. Approvals travel in env, so the kept process restarts when they change. Sub-agent tool counts and sub-agent usage are added. `add_allow_rules` is serialised with a process-wide mutex and a unique temp file.
- `mcp_manage.rs`: settings write through the same serialised path.
- `index.rs`: the `indexes_this_project` test uses a temp project. It leaves the index DB for that temp root in the user's index folder.

## 3. Still open

Implemented shared `AgentEvent::Session { session_id }`:

- OpenCode emits it after creating/resuming the server session.
- Copilot emits it after the SDK session is created/resumed.
- Antigravity emits it from the initial `init` event.
- The frontend stores it immediately, so Stop before `Done` can still resume the same provider conversation.

Frontend follow-ups for Copilot:

- `run.svelte.ts` no longer needs to mint a Copilot id: the backend now emits the provider-generated id before `Done`.
- `run.svelte.ts` `answerPermission`: Copilot cards are always treated as live. Refusal cards that arrive after the turn should follow the Claude path.
- `events.svelte.ts`: a `permission_denied` after `done` should not leave a run waiting.

Background work (not done for Copilot, Antigravity, OpenCode):

- Copilot: the SDK exposes no background task list. Auto turns after the turn ends are lost.
- Antigravity: no evidence yet of output after `result`. The tail needs a provider hook.
- OpenCode: sub-agents are synchronous. `session.background` is a user action AgentDeck never sends.

OpenCode:

- Usage cannot be streamed mid-call. Tokens appear only at `session.step.ended`.
- The serve server starts without `OPENCODE_CONFIG_CONTENT`, so OpenCode's own `question` tool is not denied. A question would stall until the watchdog ends the turn. Fix in `start()` as server-level config.
- The serve branch in `agent_runs.rs` declares `ChatResponseCapabilities::custom()`. Decide on a dedicated capability set.
- Live approval round-trip has not been run.

Antigravity:

- Stall limit: `stall.rs` `starts_sub_agent` does not name `invoke_subagent`, so a silent sub-agent gets the 2 h tool limit, not 20 min. One-line fix.
- Only `command` approvals become allow rules. Other kinds are not written.

## 3b. Additional coding-agent review

- The core model fits a desktop coding chat: one transcript may own a foreground lane plus independent listener lanes; provider events are correlated by session/turn/message identity, and ordinary shell execution is not mistaken for an idle background wait.
- The highest remaining correctness risk is provider capability mismatch, not the queue reducer. OpenCode, Antigravity and Copilot still need live checks for resume-after-stop, permission denial/approval, sub-agent completion, and late background events.
- The queued-worker cancellation gap was a real dead-thread risk and is fixed in the working tree.
- A shared session run reducer should become the single authority for send/queue/stop decisions, or be removed/rewired; keeping it beside lane state invites future divergence.
- Before release, test concurrent chats in different projects, two workers queued behind one lead, Stop during credential switching, Stop during permission wait, provider restart after a timed-out start, and a late event from the previous turn.

Copilot:

- Background work: see above.

## 4. Notes

- `markdown.ts` was restored from git once after a bad scripted edit. The diff was checked afterwards and contained only the intended changes.
- Test runner: the project has no vitest. Use `node --experimental-strip-types --test <file>` for the `.test.ts` files that have no framework.
- Frontend chat, Rust agents and rendering changes were tested mostly with fixtures in the web preview. The web preview cannot run Tauri commands, so the real desktop behaviour (permissions, workers, stop) still needs a `pnpm dev` run.
