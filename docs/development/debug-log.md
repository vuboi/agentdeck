# Debug log

Where to look when the app hangs, crashes or runs a flow wrong.

| File (under the app data folder, `logs/`) | What it holds |
| --- | --- |
| `debug.log` (`debug.1.log` is the older half) | The whole picture: every step of every turn, the Rust side's notes, the page's notes, a heartbeat, panics. Written by `src-tauri/src/flow_log.rs`. |
| `errors.log` | Only failures (a turn that failed, a tool that failed, an uncaught error in the page). `error-log.svelte.ts`. |
| `~/.agentdeck/audit.jsonl` | Tool calls that **finished** (a call that never ends leaves nothing here). |

App data folder on Windows: `%APPDATA%\dev.agentdeck.desktop\` (`...desktop.dev` for `pnpm dev`).
**Settings → Tasks → Errors drawer → Download debug log** saves one file with the version, `errors.log` and `debug.log`.

One JSON line per entry: `{at, src, chat, kind, detail}`. `src` is `rust` or `page`; `chat` is the chat id (`-` when none).
It holds no text the user wrote and none of what the agent answered (streamed text is only counted); tool inputs are cut to a line.

## Kinds

Rust side (`ag_adapters::agent::trace::note`, `flow_log::tap`):

- `turn start` / `turn end` (`why=finished|stopped|aborted`, after how long, how many events, the last kind). `aborted` means the task was dropped before it ended.
- `event`: each event sent to the chat (`tool_call`, `tool_result`, `done`, `error`, `scoped`, `task`, `auto_turn`, `permission_denied`…). `stream`: streamed text counted in 2 s windows (`delta x120 chars=4300`).
- `chat lock waited N ms`: a turn waiting for the chat's previous turn. A big number is a turn that never released it.
- `process`: a kept CLI took the turn (`pid`) or a new one started. `first line`: how long the CLI took to say anything. `turn read`: how reading ended (`park`, `tail`, `failed`, `the CLI went away` + the end of its stderr).
- `tail`: a CLI left reading after its turn (background work open), when it went on by itself (`auto-N`), how it ended (`process parked` / `process dropped, its children end with it`).
- `heartbeat` every 20 s: `running=N [chat/turn agent age=…s silent=…s events=… last=…] pool=… tails=…`. A turn whose `silent` keeps growing is the one that hung; a gap between two heartbeats means the whole process stopped.
- `PANIC`: message, thread and backtrace.

Page side (`flow-log.ts`):

- `send` (turn id, agent, model, resume), `reply end`, `event dropped` (an event of a turn that is no longer current), `queue`, `inject` (taken or not), `stop`, `render wait` / `render wait timed out`, `worker slot` (asks, granted or waits, released), `silent` (no event for a long time), `auto turn`.
- `ui blocked` (a timer fired late: the page did not run for N s), `long task` (≥ 500 ms), `window` (visible/hidden), `page started`, `page unloading`.

## Reading a hang

1. `heartbeat` lines near the time: which turn is silent, and its `last` kind.
2. That chat's lines: `chat lock waited`, `process`, `first line`, then the last `event`.
3. `ui blocked` / `long task` around the same time: the page, not the agent.
4. `render wait timed out`, `worker slot … waits in line`, `inject … not taken`: the queue and the workers.

Tests of the real CLIs are `#[ignore]`d: `cargo test -p ag-adapters --test real_codex -- --ignored --nocapture` (same for `real_agy`), and `cargo test -p agentdeck-desktop --lib copilot_real -- --ignored --nocapture`.
