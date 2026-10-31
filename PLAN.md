# AgentDeck — Master Plan

App desktop cá nhân điều phối nhiều AI agent (Claude Code, Codex, Gemini CLI / Antigravity) trong một giao diện chat kiểu Claude/Codex desktop, có preview browser nhúng, workflow multi-agent, quota/cost/fallback và audit log.

Tên tạm: **AgentDeck**. Daemon: `agentd`. CLI: `ag`.

---

## 1. Quyết định kiến trúc (đã chốt)

| Hạng mục | Chọn | Lý do |
|---|---|---|
| Shell desktop | **Tauri 2** | RAM thấp (~30–80MB), đa nền tảng, không Electron |
| Core | **Rust daemon `agentd`** tách process | Đóng UI session vẫn chạy; CLI dùng chung core |
| Giao tiếp UI ↔ core | JSON-RPC 2.0 qua WebSocket localhost (`jsonrpsee`) + token auth | UI, CLI, remote sau này dùng chung |
| Frontend | **Svelte 5 (runes)** + SvelteKit `adapter-static`, `ssr = false` | Yêu cầu user; dùng bộ UI riêng của user (tích hợp ở M4) |
| Type chung | Rust → TS bằng `specta` (`tauri-specta` / `specta-typescript`) | Không viết type tay |
| Lưu trữ | SQLite (`sqlx`) event-sourcing, append-only, hash-chain | Audit log có sẵn; resume workflow |
| Chuẩn hoá agent | **ACP** client chung + adapter native từng CLI | Viết 1 lần, fallback native khi ACP thiếu tính năng |
| Preview | Nhúng trong app. Windows: child WebView2 (Tauri multiwebview) + CDP. macOS/Linux: Chromium headless + screencast lên canvas | Không mở cửa sổ ngoài |
| CDP | `chromiumoxide` | Console, network, DOM, debugger, input |
| Workflow | YAML ⇄ canvas (`@xyflow/svelte`), CEL (`cel-interpreter`) cho điều kiện, `minijinja` cho template | Cấu hình flow kéo-thả |
| Orchestrator ↔ agent | Orchestrator là **MCP server nội bộ `hub`** inject vào mọi agent | Agent tự trao đổi qua tool chuẩn |

---

## 2. Cấu trúc repo

```
Cargo.toml                 # workspace
package.json               # pnpm workspace root
pnpm-workspace.yaml
crates/
  protocol/      # type chung: Event, Session, Quota, Flow IR... (specta export)
  store/         # SQLite event store, hash-chain, migrations
  core/          # agentd: RPC server, session manager, config (bin: agentd)
  adapters/      # acp client, claude, codex, gemini
  quota/         # QuotaWindow, ModelProfile, cost, estimate, fallback policy
  orchestrator/  # flow IR, engine, hub MCP server, scheduler
  cdp/           # PreviewBackend, CDP session, console/network/debugger
apps/
  cli/           # bin: ag
  desktop/       # Tauri 2 + Svelte 5
    src/         # SvelteKit app
    src-tauri/   # Tauri Rust side (spawn/connect agentd, multiwebview preview)
packages/
  protocol-ts/   # TS types sinh từ crates/protocol
  ui/            # placeholder → thay bằng bộ UI của user
```

---

## 3. Model dữ liệu cốt lõi (crates/protocol)

```
Project { id, name, path }                       # a working folder; sessions group under it
Session { id, project_id, title, agent: AgentKind, model, created_at,
          git_branch?, parent_id?, fork_seq? }   # parent_id + fork_seq = forked from parent at that event
Turn    { id, session_id, role: user|agent, started_at, ended_at }
Event   { seq, id, session_id, turn_id?, ts, kind: EventKind, prev_hash, hash }
EventKind =
  MessageDelta{text} | Thinking{text} |
  ToolCall{call_id, name, input, status: pending|running|ok|fail, output?} |
  PermissionRequest{...} | Diff{path, patch} | TerminalOutput{chunk} |
  Usage{tokens_in, tokens_out, cache_read, cache_write, cost_usd?} |
  AgentHandoff{from, to, artifact_id} | Artifact{id, kind, schema, content} |
  ModelSwitch{from, to, reason, quota_snapshot} | StatusChange{state}
AgentKind = claude | codex | gemini | antigravity
QuotaWindow  { provider, account, kind: h5|weekly|daily|rpm|tpm, used, limit, unit: pct|tokens|requests, resets_at, source: exact|estimated|reactive, observed_at }
ModelProfile { id, provider, tier, context_window, max_output, price_in, price_out, price_cache_read, price_cache_write, tps_measured }
```

---

## 3b. Session tree, forks, mentions and history import

**Session tree (sidebar).** `Project > Session > Fork`, rendered with `ZTree`. There is no "branch"
level: a *branch* here means a fork of a chat session, not a git branch.
- **Project**: a folder on disk (`cwd`). Sessions group by `project_id`; a project can be collapsed.
- **Session**: one conversation with one agent + model. Its git branch, if any, is just metadata
  (`git_branch`, shown as a small tag; each coding session gets its own worktree in M5).
- **Fork**: what Claude Code and Codex call forking a conversation, so a long chat does not have to
  carry everything forever. A fork copies the parent history up to `fork_seq` into a new session
  (`parent_id` = parent, `fork_seq` = last shared event) and continues on its own, possibly with
  another agent/model. Forks nest under their parent and can be forked again. The fork gets a short
  handoff summary instead of the whole transcript when it switches provider (token saving).
- Row: status dot (working / waiting / idle), title, agent. Hover shows Fork.

**Composer.**
- Agent and model are picked with `ZSelect` (model list depends on the agent).
- `@` opens a mention palette (`ZCommand`) with three groups: **Sessions** (inject a short summary of
  that session), **Projects** (extra path as context) and **AI** (route this message to an agent + model).
- Mentions show as removable chips; the daemon resolves them into the context pack on send.
- Later: `@file` (fuzzy index), `/command`, `#skill`.

**Tools.** Right-panel tools (Preview, Diff, Workflow, Usage, Audit) can pop out into a `ZModal` and
dock back; the active tool is shared state, so the modal and the panel always show the same view.

**History import (read-only).** A `history` module reads what Claude Code and Codex already wrote to
disk, so the user can switch to AgentDeck and still see old chats in the same UI. Nothing is ever
written to those folders.

| Source | Location | Unit | Notes |
|---|---|---|---|
| Claude Code (CLI and desktop Code tab) | `~/.claude/projects/<cwd with separators replaced by "-">/<session-uuid>.jsonl` | one file per session | Subagents live in `agent-*.jsonl` (`isSidechain: true`). |
| Codex (CLI and desktop) | `~/.codex/sessions/YYYY/MM/DD/rollout-<timestamp>-<uuid>.jsonl` | one file per session | First line is `session_meta`. |

Claude Code lines (observed): `user`, `assistant`, `system`, `attachment`, `ai-title` / `custom-title`
(session title), `last-prompt` (`leafUuid`), `file-history-snapshot`, `queue-operation`, `permission-mode`,
`cost-state`. Each message line has `uuid`, `parentUuid` (the conversation is a tree: two children of one
parent = a rewind or fork point), `sessionId`, `cwd`, `gitBranch`, `timestamp`, `isSidechain`. Assistant
`message.content` items are `text`, `thinking`, `tool_use`; user ones are text or `tool_result`.

Codex lines (observed): `session_meta` (`id`, `cwd`, `git`, `cli_version`, `source`, `model_provider`),
`turn_context`, `response_item` (`message`, `reasoning`, `function_call` / `function_call_output`,
`custom_tool_call` / `custom_tool_call_output`), `event_msg` (`task_started`, `task_complete`,
`token_count`, `turn_aborted`), `compacted`.

Mapping into `EventKind`: assistant text -> `MessageDelta`; thinking / reasoning -> `Thinking`; tool
use + result -> `ToolCall` (status from the result); token usage -> `Usage`; Codex `compacted` -> a
`StatusChange`. Titles come from `ai-title` / `custom-title`; project from `cwd`; a session whose
`uuid` chain shares a prefix with another becomes a fork (`parent_id`, `fork_seq`).

Rules: tolerant parser (unknown line types are skipped and counted, never fatal); fixtures recorded from
real files with the text redacted; lazy loading (index titles and timestamps first, parse bodies on open);
formats are not documented by the vendors, so pin the CLI versions seen and keep contract tests.

---

## 4. Module & thứ tự làm

Mỗi module có Definition of Done (DoD). Làm tuần tự, mỗi module = 1 commit (hoặc vài commit) sau khi review pass.

### M0 — Scaffold monorepo
- Cargo workspace với tất cả crate ở mục 2 (stub compile được, `lib.rs` có 1 test mẫu).
- pnpm workspace; `apps/desktop` = Tauri 2 + SvelteKit + Svelte 5 + TS + `adapter-static`.
- Layout skeleton kiểu Codex/Claude desktop: titlebar tự vẽ (`decorations: false`, `data-tauri-drag-region`), sidebar trái (session), cột chat giữa (max-width ~760px) + composer đáy, panel phải resize được với tab rỗng: Preview / Diff / Terminal / Workflow / Usage / Audit. Dữ liệu mock.
- Tooling: `rustfmt.toml`, `clippy` sạch, Prettier + `svelte-check`, `.editorconfig`, `.gitignore`.
- Script root: `pnpm dev` (tauri dev), `pnpm check` (cargo check + clippy + svelte-check), `pnpm test`.
- **DoD**: `cargo check --workspace`, `cargo clippy --workspace -- -D warnings`, `cargo test --workspace`, `pnpm -r check`, `pnpm --filter desktop build` đều pass.

### M1 — Protocol + Event store
- Type ở mục 3 trong `crates/protocol`, serde + specta; export TS sang `packages/protocol-ts`.
- `crates/store`: SQLite (`sqlx`, migrations), append event, hash-chain (sha256 của prev_hash + payload), verify chain, query theo session/turn, JSONL export.
- **DoD**: unit test append/query/verify, phát hiện sửa đổi event.

### M2 — agentd + CLI
- `agentd`: JSON-RPC qua WebSocket localhost, token trong file config user-dir; method: `session.create/list/get/send/cancel`, `events.subscribe` (stream).
- Session manager quản lý process agent (spawn, kill, PTY nếu cần).
- `ag`: `ag sessions`, `ag new --agent claude`, `ag send <id> "msg"`, `ag tail <id>`.
- Desktop: Tauri side spawn `agentd` nếu chưa chạy, rồi UI kết nối WS.
- **DoD**: chạy agentd + `ag` với adapter `echo` giả lập stream event.

### M3 — Adapter Claude + ACP client
- Trait `AgentAdapter { start, send, cancel, set_model, events() }`.
- Claude: `claude -p --input-format stream-json --output-format stream-json --verbose`, `--resume`, `--model`, `--mcp-config`; parse → EventKind.
- ACP client generic (JSON-RPC stdio) dùng cho `claude-code-acp` / `codex-acp` / `gemini --experimental-acp`.
- **DoD**: contract test với fixture JSONL đã ghi; chat thật 1 lượt với Claude qua `ag`.

### M4 — UI chat
- Store Svelte 5 (`$state` class) cho sessions/events; batch update theo rAF.
- Timeline virtualized, markdown + Shiki, tool card (collapsible, spinner, thời gian), terminal stream (xterm.js), diff (CodeMirror merge), status badge thinking/working/running cmd/waiting approval.
- Composer: `ZSelect` agent/model/effort; inline mentions for chats/projects/models (`@` palette or `/c`, `/p`, `/m` shortcuts), `@file` (fuzzy từ core), `/command`, đính ảnh. Model mentions stay in the prompt and ask the main model to use AgentDeck's `delegate_task` sub-agent tool.
- Sidebar: cây `ZTree` Project > Session > Fork (fork lồng dưới session cha); tạo fork từ menu session; trạng thái working/waiting/idle.
- Tích hợp bộ UI của user (thay `packages/ui`).
- **DoD**: chat Claude end-to-end trong app, stream mượt, permission prompt hoạt động.

### M3b — History import (đọc lịch sử Claude/Codex)
- Module `history` trong `crates/adapters`: reader Claude Code (`~/.claude/projects/**/<uuid>.jsonl`) và Codex (`~/.codex/sessions/**/rollout-*.jsonl`), map sang `EventKind` (xem mục 3b).
- Index nhanh (title, cwd, thời gian) rồi parse thân khi mở; session import là read-only, hiển thị cùng format với session thật; fork nhận diện qua chuỗi `parentUuid`.
- **DoD**: test với fixture đã ẩn nội dung; import 1 session Claude + 1 session Codex thật và xem trong UI.
- **Trạng thái**: reader Claude + Codex xong (`crates/adapters/src/history`), lệnh Tauri `history_list` / `history_load` (chỉ nhận path nằm trong 2 thư mục history), sidebar và chat hiển thị lịch sử thật. Fork nhận diện bằng cách hai session chung uuid tin nhắn đầu (resume/fork của Claude Code); sau compaction thì giữ phần tin nhắn cũ trước điểm đứt `parentUuid`. Chạy thử trên máy thật: `cargo run -p ag-adapters --example history_stats` (chỉ in số lượng, không in nội dung). Codex chưa thấy dấu hiệu fork trong file, nên chưa nhận diện.

### M5 — Codex + Gemini + MCP + worktree
- Codex: `codex app-server` (JSON-RPC) hoặc `codex-acp`; Gemini: ACP. Antigravity: verify khả năng headless, nếu không có thì đánh dấu deferred.
- MCP manager tập trung → dịch sang config từng CLI.
- Git worktree per coding session.
- **DoD**: 3 agent chat được trong app; MCP server khai báo 1 lần dùng được ở cả 3.

### M6 — Quota / cost / estimate
- Thu thập quota 3 tầng: exact (header/event CLI), estimated (event store + plan caps cấu hình), reactive (parse lỗi limit).
- Bảng giá + ModelProfile trong config versioned.
- Pre-flight estimate P50/P80 (token, $, thời gian) từ `RunStats`; verdict đủ/sát/không đủ; burn rate + time_to_limit khi đang chạy.
- UI: tab Usage (card %, countdown reset, badge exact/est, sparkline), chip ước lượng ở composer, status bar.
- **DoD**: số liệu Codex rate_limits + Claude usage hiển thị đúng; test đơn vị cho estimator.

### M7 — Preview nhúng
> **Trạng thái (2026-10-22): bản đầu đã chạy, dùng cách nhẹ hơn CDP.** Child webview (Tauri multiwebview, `unstable`) đặt đè lên panel Preview;
> script chèn vào mọi trang (`apps/desktop/src-tauri/browser/inject.js`) báo console / network / điều hướng và trả lời yêu cầu DOM, screenshot
> (html-to-image), element picker, gửi về app qua URL scheme `agentdeck` -> event `browser-report`. UI: thanh địa chỉ, thiết bị Mobile/Tablet/Desktop
> (zoom để đúng bề rộng), pick element, Console/Network, Annotate (freeze-frame: mũi tên, ô vuông, nét vẽ, note đánh số) rồi "Add to chat"
> (ảnh + DOM + lỗi console vào composer). Chưa có: CDP/chromiumoxide, debugger, network body, tool browser cho agent qua MCP, macOS/Linux chưa thử.
- Guide authoring riêng trong Preview: lưu các bước ảnh kèm mô tả/chú thích (tô vùng, khung, tròn, mũi tên, chữ), quay luồng thao tác thành video có lớp highlight, chỉnh zoom và xuất một HTML tự chứa.
- Trait `PreviewBackend`; impl `Webview2Native` (Tauri `add_child`, `--remote-debugging-port=0`, đọc `DevToolsActivePort`, `chromiumoxide`) và `CdpScreencast` (headless Chromium + canvas).
- Console, network (kèm body), element picker (isolated world), annotate kiểu freeze-frame, context chip gửi vào chat (selector, outerHTML, computed style, box, ảnh crop, source file nếu có).
- Tool browser expose cho agent qua hub MCP: click/type/screenshot/read_dom/console/network.
- **DoD**: mở localhost app trong panel, chọn element → chip vào chat, agent đọc được console/network.

### M8 — Orchestrator engine + fallback
- Flow IR, parse/validate YAML (`.agentflow/*.flow.yaml`), node types: trigger/agent/logic/data/check/human/system.
- Engine durable trên event store (resume sau crash), concurrency theo provider, budget.
- Hub MCP server: `post_artifact`, `ask_agent`, `request_review`, `get_context`, `report_status` + tool browser.
- Fallback engine: chain theo role, triggers (quota/429/529/stall/context/budget), circuit breaker, handoff pack khi đổi provider, `min_tier`, cooldown, switch back sau reset.
- Scheduler: chạy sau reset quota, cron.
- **DoD**: flow `feature` mẫu chạy được bằng `ag flow run`; test fallback giả lập 429.

### M9 — Workflow canvas
- `@xyflow/svelte`, node = component Svelte, inspector sinh form từ JSON Schema, đồng bộ YAML ⇄ canvas qua IR.
- Run view: trạng thái node, badge token/$/model, edge animation, click node → transcript.
- Debug: breakpoint node, pin output, rerun từ node, pre-flight cả flow.
- **DoD**: tạo flow bằng kéo thả, chạy, xem trạng thái realtime.

### M10 — Plugin / skill / debugger
- Plugin manifest (mcpServers, skills SKILL.md, commands, hooks, roles, flows, node types).
- Debugger: logpoint (`Debugger.setBreakpointByUrl` + condition), capture scope khi pause.
- Audit tab: lọc, verify chain, export.

---

## 5. Quy trình build bằng multi-agent (để tiết kiệm token)

| Vai | Model | Việc |
|---|---|---|
| Planner | Claude Opus | Viết/cập nhật PLAN.md, brief từng module |
| Điều phối | Claude Sonnet | Đọc brief, giao Codex, chạy lệnh DoD, tổng hợp |
| Coder | Codex `gpt-6-luna` (`codex exec -m gpt-6-luna`) | Viết code module |
| Reviewer | Claude Sonnet, effort medium | Review diff module: đúng, an toàn, khớp PLAN |

Vòng mỗi module: brief → Codex code → DoD check → Sonnet review → Codex fix (tối đa 2 vòng) → commit `feat(Mx): ...`.

Quy ước code cho mọi agent: xem `AGENTS.md`.

---

## 6. Đợt tiếp theo (2026-10-23): quota, Antigravity, MCP, terminal thật

Thứ tự: **M11 → M12 → M13 → M14**. M11 và M14 độc lập nhau, làm song song được (hai nhánh khác file). M13 làm sau M12 vì cần
cả ba CLI. Mỗi module có brief đủ để giao thẳng cho Claude hoặc Codex (`codex exec -m gpt-6-luna`); phần "Gợi ý giao" nói
nên giao ai. Quy ước chung vẫn là `AGENTS.md`. Mục "Cần verify" là việc đầu tiên của module: chạy thử CLI thật, ghi fixture
(đã ẩn nội dung) rồi mới viết parser.

### M11 — Trạng thái giới hạn token (Claude + Codex)

**Mục tiêu.** Thấy ngay còn bao nhiêu quota của từng provider, bao lâu nữa reset, và có đủ cho việc sắp làm không. Đây là
phần "exact" của M6; phần estimate/fallback vẫn ở M6/M8.

**Nguồn dữ liệu (đã kiểm tra trên máy này).**

| Provider | Nguồn | Dạng |
|---|---|---|
| Codex | Dòng `event_msg` / `token_count` trong rollout `~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl`, trường `rate_limits` | `primary` (`window_minutes: 300` = cửa sổ 5h) và `secondary` (`10080` = tuần): `used_percent`, `resets_at` (epoch giây); thêm `plan_type`, `credits`, `rate_limit_reached_type` |
| Claude | Claude Code đưa `rate_limits.five_hour` / `rate_limits.seven_day` (`used_percentage`, `resets_at`) vào JSON stdin của **statusline** (chỉ ở phiên tương tác, gói subscription). Máy này đã có tool bên thứ ba (AI Usage Meter) ghi chúng vào `~/.claude/usage-cache.json` | `%` đã dùng + giờ reset |

**Việc cần làm.**
1. `crates/quota`: `QuotaWindow` đúng như mục 3 (`provider, account, kind, used, limit, unit, resets_at, source, observed_at`), `QuotaSnapshot { provider, plan, windows[], credits? }`. `source = exact | estimated | reactive`.
2. Reader **Codex**: `crates/adapters/src/quota/codex.rs` đọc rollout mới nhất, lấy `rate_limits` của dòng `token_count` cuối cùng (đọc từ cuối file, không parse cả file). Khi agent Codex đang chạy trong app, lấy thẳng từ stream nếu `codex exec --json` cũng phát event này (verify).
3. Reader **Claude**, theo thứ tự ưu tiên, dừng ở nguồn đầu tiên có dữ liệu mới (< 10 phút):
   a. `~/.claude/usage-cache.json` nếu có (`fetchedAt`, `rate_limits`);
   b. event `rate_limit_event` trong stream-json của `claude -p` nếu CLI có phát (verify);
   c. reactive: parse lỗi/limit-reached của turn (`source = reactive`, chỉ biết "đã chạm giới hạn, reset lúc X").
   **Không** tự sửa `~/.claude/settings.json` (statusline của user đang do tool khác giữ). Không gọi API OAuth không công bố trừ khi user bật tay trong Settings, vì token nằm trong `~/.claude/.credentials.json`.
4. Lệnh Tauri `quota_get` (đọc ngay) + event `quota-update` (đẩy khi file đổi, dùng `notify`; debounce 1s). Không poll mạng.
5. Cache snapshot cuối cùng vào prefs để mở app là có số ngay, gắn nhãn `observed_at`.
6. **UI** (tab `Usage` đang trống trong `tools.ts`/`ToolView.svelte`):
   - Mỗi provider một card: tên + plan, hai thanh tiến trình (5h, tuần) theo `%`, countdown tới reset ("resets in 2h 14m", cập nhật qua `ticker`), badge `exact` / `est` / `stale` (khi `observed_at` > 15 phút).
   - Màu theo ngưỡng (xanh < 70%, vàng 70–90, đỏ ≥ 90); thanh chạm 100% thì hiện "đang bị giới hạn, reset lúc …".
   - Dưới card: tiêu thụ thật từ `usageLog` hiện có (`usage.ts`: token, cost) theo ngày/tuần, sparkline.
   - Chip nhỏ ở composer / status bar: `5h 34% · 7d 12%` của agent đang chọn, bấm mở tab Usage. Khi ≥ 90%, composer cảnh báo trước khi gửi.
   - Trạng thái trống: provider chưa đăng nhập hoặc chưa có dữ liệu thì card nói rõ vì sao và cách có dữ liệu, không để thanh 0%.
7. Test đơn vị: parse fixture Codex và Claude (có cả trường thiếu/null), tính countdown, ngưỡng màu. Không gọi network.

**Cần verify.** Codex `exec --json` có phát `token_count` không; stream-json của `claude -p` có `rate_limit_event` không; `resets_at` Claude là giây hay mili giây.

**DoD.** Số trên tab Usage khớp với `/status` của Codex và statusline của Claude tại cùng thời điểm; đóng/mở app vẫn có số (stale); `cargo test -p ag-quota -p ag-adapters`, clippy, `svelte-check` sạch.

**Gợi ý giao.** Reader + test Rust: Codex. UI card/chip/trạng thái trống: Claude (cần gu thiết kế, bám `z-ui-svelte`).

### M12 — Antigravity (`agy`) như provider thứ ba

**Hiện trạng.** Trait `Provider` (`crates/adapters/src/agent/mod.rs`) đã đủ để thêm CLI mới: một impl = `args()` + `parse_line()`, UI chỉ thấy `AgentEvent`. `AgentKind` trong protocol đã có `antigravity`. `models.svelte.ts` đang có provider placeholder `gemini` (model giả) và `provider_for` trong `lib.rs` chỉ biết `claude`/`codex`.

**CLI thật (đã chạy thử, `agy` 1.4.5, `%LOCALAPPDATA%\agy\bin\agy.exe`).**
- Một lượt: `agy -p "<prompt>" --output-format stream-json --model <id>`; nhiều lượt trong một process: `--input-format stream-json` (NDJSON trên stdin).
- Cờ dùng được: `--model`, `--effort low|medium|high|xhigh|max`, `--mode accept-edits|auto|plan`, `--dangerously-skip-permissions`, `--conversation <id>` (resume), `--continue`, `--add-dir`, `--project`, `--sandbox`, `--print-timeout`.
- `agy models` in `<id>\t<label>`; id đã gồm mức effort (`gemini-4-argon-high`, `claude-opus-5-5-medium`, `gpt-oss-120b-medium`…), và có cả model Claude/GPT-OSS chạy qua Antigravity.
- `agy mcp add|remove|list|enable|disable` quản lý MCP (dùng ở M13).
- Stream **không giống Claude**, mỗi dòng là một JSON có `event`:
  - `{"event":"init","conversation_id","init":{"model","cwd","tools":[...]}}`
  - `{"event":"step_update","step_update":{"step_index","state":"ACTIVE|DONE","step_type":"user_input|agent_response|…","text_delta","duration_seconds","usage":{input_tokens,output_tokens,thinking_tokens,cache_read_tokens,total_tokens}}}`
  - `{"event":"result","result":{"conversation_id","status":"SUCCESS|…","response","duration_seconds","num_turns","usage"}}`
- Tool list của agy có `browser_*`, `ask_permission`, `ask_question`, `call_mcp_tool`, … (agy có browser riêng; xem rủi ro cuối mục).

**Cần verify (làm trước, ghi fixture vào `crates/adapters/tests/fixtures/agy/`).** Chạy `agy` với prompt cần tool (đọc file, sửa file, chạy shell, hỏi quyền) và ghi lại `step_type` của bước tool call/tool result, cách báo lỗi (`status` khác SUCCESS), cách yêu cầu quyền khi không có `--mode`, có event thinking không, có hiện quota/limit của Antigravity không (xem `~/.gemini/antigravity-cli`).

**Việc cần làm.**
1. `crates/adapters/src/agent/antigravity.rs`: `struct Antigravity` impl `Provider` (`id: "antigravity"`, `binary: "agy"`, `candidates`: `%LOCALAPPDATA%\agy\bin\agy.exe`, nhớ `.exe` trên Windows như `resolve_binary` đã làm).
2. `args()`: map `TurnRequest` → cờ (`--conversation` khi `resume`, `--model`, `--effort`, `--mode` từ `permission_mode`, `--add-dir`/cwd, `--output-format stream-json`). Bảng map `permission_mode` ↔ `--mode` có test.
3. `parse_line()`: `step_update` → `Delta` (text_delta của `agent_response`), `Thinking`, `ToolCall`/`ToolResult` (id = `step_index`, `change` cho tool sửa file qua `file_change` hiện có), `Usage` (`new_call` khi step mới bắt đầu), `result` → `Done { session_id: conversation_id, text: response, cost_usd: None }`, `status` lỗi → `Error`. Parser có state nhỏ (ghép delta theo `step_index`): dùng `Mutex` trong struct như `Codex::default()` đã làm.
4. `models.rs`: `antigravity(home)` chạy `agy models` (timeout 10s, cache theo mtime của binary) → `ModelInfo`; gộp các id cùng gốc khác hậu tố `-low/-medium/-high` thành một model với `efforts` (picker đã có mục effort). Nếu `agy` không có hoặc lỗi thì danh sách rỗng + báo lý do, không dùng model giả.
5. `lib.rs`: `provider_for("antigravity")`, `models_list`; UI: đổi `ProviderId` `'gemini'` thành `'antigravity'` trong `models.svelte.ts` (icon, nhãn "Antigravity"), migrate session đã lưu có `agent: 'gemini'` sang `antigravity` khi hydrate (như đã làm với tool `Audit` thành `Tasks` trong `layout.svelte.ts`).
6. Browser tools: agy có `browser_*` riêng nên **không** tự gắn browser MCP của AgentDeck mặc định; cho bật ở M13 (`agy mcp add`). Ghi rõ trong tooltip để user không nhầm hai trình duyệt.
7. History import (M3b) cho `~/.gemini/antigravity-cli` **không** nằm trong module này (chưa rõ định dạng); chỉ ghi lại vị trí file trong verify.
8. Test: fixture JSONL thành chuỗi `AgentEvent` (text, tool, lỗi, nhiều lượt), `args()` từng tổ hợp, models parser với output `agy models` mẫu.

**DoD.** Chọn Antigravity trong picker, chat một lượt thật có tool + dừng + resume bằng `--conversation`; model list lấy từ `agy models`; `cargo test -p ag-adapters`, clippy sạch, `svelte-check` sạch; Tasks và Usage ghi đúng lượt của agy.

**Trạng thái (2026-10-23): xong.** `crates/adapters/src/agent/antigravity.rs`, `models::antigravity`, `provider_for`/`models_list` trong Tauri, UI đổi `gemini` thành `antigravity` (chat đã lưu tự chuyển). Đã verify bằng `agy` 1.4.5: tool step có `tool_name` + `tool_info.parameters` nhưng **không có nội dung file** (nên không có diff cho Write/Edit); không có event thinking; model đã có hậu tố mức effort, `--model <base> --effort <level>` chạy được còn `--model <id-có-hậu-tố> --effort` báo xung đột; `--conversation` với id lạ chỉ cảnh báo rồi mở hội thoại mới; `--mode auto` không dùng được ở tài khoản này (agy tự về mode mặc định); ở chế độ mặc định một tool cần quyền (ghi file) có thể treo, nên provider luôn thêm `--print-timeout 30m`; lệnh shell bị từ chối được báo ở `denied_actions` của `result`, agy không nói lệnh nào. Duyệt quyền: agy không có cờ allow, chỉ có luật `permissions.allow` trong `~/.gemini/antigravity-cli/settings.json`, khớp **chính xác** từng lệnh (đã thử: `command(echo hi)` không cho `echo hi there` hay `echo hi; echo evil`, và `*` không phải glob). Lệnh bị từ chối được lấy từ step tool (`denied_actions` chỉ báo loại), thẻ duyệt trả `agy:command(<lệnh>)`, và `Provider::prepare` ghi đúng luật đó vào settings trước lượt chạy lại (giữ phần còn lại của file, sao lưu một lần ra `settings.json.agentdeck.bak`, file hỏng thì không đụng). Lệnh có xuống dòng hoặc dấu ngoặc không thể viết thành luật an toàn nên không có thẻ duyệt. `--dangerously-skip-permissions` chỉ dùng khi user chọn chế độ bypass. Chạy thật: `cargo run -p ag-adapters --example agy_turn -- "<prompt>" gemini-3.8-flash` (lượt 2 resume lượt 1).

**Gợi ý giao.** Codex (Rust thuần, theo mẫu `codex.rs`/`claude.rs`); Claude chỉ làm phần picker/migrate UI nếu muốn tách.

### M13 — Quản lý MCP tập trung

**Hiện trạng.** `mcp.rs` chỉ là MCP server của browser (AgentDeck là server). `claude.rs` inject bằng `--mcp-config`, `codex.rs` bằng `-c mcp_servers.agentdeck.url=…`. Chưa có chỗ user khai báo MCP server của họ.

**Việc cần làm.**
1. Model `McpServer { id, name, transport: stdio{command,args,env,cwd} | http{url,headers}, enabled, scope: global | project(path), agents: AgentKind[] }` lưu trong `~/.agentdeck/mcp.json` (secret trong `env`/`headers` lưu qua keyring của OS, file chỉ giữ tham chiếu; không commit).
2. **Dịch ra từng CLI** (`crates/adapters/src/mcp_config.rs`, hàm thuần, test bằng snapshot):
   - Claude: JSON `--mcp-config` (gộp với server `agentdeck` hiện có);
   - Codex: các cờ `-c mcp_servers.<name>.…` hoặc đoạn `config.toml` tạm;
   - Antigravity: `agy mcp add/remove/list` (CLI tự giữ trạng thái, nên AgentDeck đồng bộ: so `agy mcp list` với bảng của mình rồi add/remove phần lệch). Verify cú pháp `add`.
3. **Nhập sẵn**: đọc (chỉ đọc) MCP đã có của user từ `~/.claude.json`/`.mcp.json`, `~/.codex/config.toml`, `agy mcp list`, hiện dạng "Nhập vào AgentDeck" từng cái.
4. **Kiểm tra sống**: nút "Test" khởi MCP server (stdio hoặc HTTP) gọi `initialize` + `tools/list`, hiện số tool, lỗi và thời gian; timeout 10s; chạy ngoài luồng UI.
5. **UI** (Settings hoặc tab mới `MCP`): danh sách server, bật/tắt theo agent, form thêm (stdio/http, env, header), trạng thái (ok/lỗi/chưa test), nút nhập, xoá có xác nhận. Server browser của AgentDeck hiện ở đầu danh sách, chỉ cho bật/tắt từng agent.
6. `TurnRequest` thêm `mcp: Vec<McpServer>` đã lọc theo agent + project; `agent_send` đọc từ kho.
7. Hub MCP của orchestrator (M8) dùng lại đúng kho này.

**DoD.** Khai báo một MCP server một lần, dùng được ở cả Claude, Codex và Antigravity trong một chat thật (agent gọi được tool của nó); test snapshot cho ba bản dịch; không secret nào nằm trong log hoặc `mcp.json` dạng rõ.

**Gợi ý giao.** Bản dịch + test + kho: Codex. UI quản lý: Claude.

### M14 — Terminal thật (nhập lệnh, xem log)

**Hiện trạng.** `Terminal.svelte` + `terminal.svelte.ts` là mock: `lines: string[]`, mỗi lệnh chỉ in dòng "(mock) command queued". Không có `xterm`, không có PTY.

**Việc cần làm.**
1. Rust: `portable-pty` (thêm vào `[workspace.dependencies]`). Lệnh Tauri `pty_open { id, cwd, shell?, cols, rows }`, `pty_write { id, data }`, `pty_resize { id, cols, rows }`, `pty_close { id }`; output đẩy bằng event `pty-data { id, data }` (gộp theo ~8ms hoặc 16KB để không nghẽn IPC), `pty-exit { id, code }`. Shell mặc định theo OS (Windows: `pwsh` nếu có, không thì `powershell`; Unix: `$SHELL`). Dọn process khi đóng tab và khi thoát app (không để shell mồ côi).
2. Frontend: `@xterm/xterm` + `@xterm/addon-fit` + `@xterm/addon-webgl` (rơi về canvas nếu lỗi) + `addon-web-links`; thay vùng log mock bằng xterm; `ResizeObserver` gọi fit rồi `pty_resize`. Màu lấy từ token theme hiện có (`theme-tokens.ts`), đổi theo sáng/tối. Giữ tab, phím tắt, animation mở/đóng đang có.
3. Mỗi tab một PTY, mở theo `cwd` của project đang chọn; đóng tab = kill PTY. Scrollback giữ trong xterm (10k dòng); lưu lên prefs chỉ 500 dòng cuối để "khôi phục nhìn" (không phải khôi phục process) và ghi rõ trên UI là shell mới.
4. **Xem log của task nền**: tab terminal chỉ-đọc cho từng dev server do `browser_serve` khởi (server do AgentDeck giữ, `servers.rs`): cùng xterm nhưng không nhận input, có nút "Dừng". Từ một dòng trong Tasks (shell nền) bấm "Xem log" mở đúng tab đó.
5. Tính năng nhỏ nhưng cần: copy/paste (Ctrl+Shift+C/V, chuột phải), tìm trong terminal (`addon-search`, Ctrl+F khi đang focus), xoá màn hình, đổi tên tab, "gửi lựa chọn vào chat" (chọn text thì thêm vào composer).
6. Bảo mật: terminal chạy với quyền của user như mọi terminal; chỉ cửa sổ chính được gọi lệnh `pty_*` (capability của Tauri), `pty_open` không nhận đường dẫn shell tuỳ ý từ nội dung trang web trong Preview.
7. Test: Rust test mở PTY chạy `echo`, đọc output, resize, close (trên Windows dùng `cmd /c echo`); không test network.

**DoD.** Gõ lệnh tương tác thật (`pnpm dev`, `git log`, `python` REPL) chạy đúng, màu ANSI đúng, resize không vỡ dòng, đóng tab không còn process con (kiểm tra bằng Task Manager), tab log của dev server hiện output đang chạy; `cargo clippy`, `svelte-check` sạch.

**Gợi ý giao.** PTY Rust + event: Codex. Tích hợp xterm/theme/tab/log nền: Claude.

### M15 — Provider platform: facade, manifest và routing profile

**Mục tiêu.** Mở rộng provider theo từng giao thức, giữ nguyên command/frontend contract hiện tại và không đổi provider giữa một stream đang chạy.

**Trạng thái (2026-10-31): nền tảng đầu đã có.** WorkBuddy đã tách thành connector/provider module; provider preset có Cloudflare, DeepSeek, Kimi, GLM, MiniMax, Command Code và Alibaba Qwen; routing profile đã có schema/persistence và quota threshold cơ bản.

**Việc cần làm theo thứ tự.**
1. Hoàn tất facade `connectors/` và `providers/`: chuyển từng connector lớn vào module riêng (`claude`, `codex`, `antigravity`, `copilot`, `opencode`, `api_key`, `secrets`) nhưng giữ Tauri command ở facade để frontend không phải migrate một lần.
2. Thêm `ProviderManifest` dùng chung: `id`, `auth`, `protocol`, `capabilities`, `quota`, `free_tier`, `risk_policy`, `image_backend` và `search_backend`. Registry là nguồn duy nhất cho model picker, Settings, workflow và image generation.
3. Tách routing thành ba tầng: lọc capability/plan/cost, chấm điểm health/quota/latency, rồi retry/fallback. Một session được pin provider/account trong turn; fallback chỉ xảy ra tại retry boundary an toàn.
4. Hoàn thiện các profile AgentDeck: `auto`, `coding`, `fast`, `subscription`, `thrifty`, `offline`, `lkgp`. Mọi quyết định phải lưu lý do (`quota thấp`, `không có tool`, `cooldown`, `latency`) để UI hiển thị được.
5. Dùng quota `exact`, `estimated`, `reactive` hiện có; `subscription` không rơi âm thầm xuống free, `offline` không gọi network để probe quota.

**DoD.** Có manifest registry duy nhất; chat cũ vẫn resume được; route decision có test cho capability/quota/cooldown/pinning; `429`, timeout và stream lỗi chỉ fallback ở ranh giới an toàn; `cargo clippy`, `cargo test`, `svelte-check` sạch.

### M16 — Provider API ổn định, search và image generation

**Mục tiêu.** Có một pool nhỏ, chất lượng và dễ kiểm soát thay vì tích hợp hàng loạt free endpoint không ổn định.

**Ưu tiên tích hợp.**
1. API-compatible: Cloudflare Workers AI, DeepSeek, Kimi/Moonshot, GLM/Z.AI, MiniMax, Command Code, Alibaba Qwen và OpenCode Zen nếu endpoint/auth ổn định.
2. Free/no-auth có kiểm soát: Pollinations và GitHub Models; mỗi provider có quota/cooldown/risk policy riêng, không coi free là quota chung.
3. Search: Brave Search API làm adapter chính, HTML engines chỉ là fallback; key đọc từ environment/secret store, không đưa vào prompt, log hoặc export.
4. Image: dùng backend riêng cho `/images/generations`, Gemini image API và provider native nào có API ảnh đã xác minh. Chat provider chỉ được xuất hiện trong Image Generator khi có capability `image_generation`.

**DoD.** Provider có key test/probe, model discovery, streaming chat, error mapping, quota source và logo; Search trả kết quả có URL/snippet; Image Generator lưu được ảnh base64 và báo lỗi provider rõ ràng; không gọi network từ test.

### M17 — CLI/OAuth và provider experimental

**Mục tiêu.** Thêm provider có auth riêng mà không nhét credential vào generic custom provider.

**Việc cần làm.**
1. Command Code: preset API đã có; tiếp tục xác minh OAuth flow, session header rotation, resume và tool event trước khi viết connector native.
2. Muse Spark Web: connector cookie trong keyring, executor riêng, bật cờ `experimental`; không đưa vào `auto` mặc định, không dùng cho dữ liệu dự án khi chưa có risk policy.
3. Kiro và provider reverse/web khác chỉ bật opt-in sau khi kiểm tra điều khoản, độ ổn định và khả năng xoá credential.

**DoD.** Login/refresh/logout không lộ token; stream/tool/error map về `AgentEvent`; session pin/fallback đúng policy; UI hiển thị experimental và lý do provider bị loại khỏi auto routing.

### Rủi ro chung
- Định dạng của `agy`, `~/.claude/usage-cache.json`, rollout Codex đều **không được vendor công bố**: ghim phiên bản đã thấy (agy 1.4.5, Claude Code 2.1.306), parser dung thứ (dòng lạ bỏ qua và đếm), có contract test từ fixture.
- Trình duyệt kép: Antigravity có `browser_*` của riêng nó; để mặc định tắt cho tới khi M13 cho chọn rõ.
- M11 phụ thuộc vào việc user dùng gói subscription (API key thuần không có `rate_limits`): UI phải nói rõ "không áp dụng" thay vì để trống.
