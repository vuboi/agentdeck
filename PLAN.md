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
Session { id, project_id, title, agent: AgentKind, model, branch, created_at,
          parent_id?, fork_seq? }                # parent_id + fork_seq = forked from parent at that event
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

## 3b. Session tree, forks and mentions

**Session tree (sidebar).** `Project > Branch > Session > Fork`, rendered with `ZTree`:
- **Project**: a folder on disk. Sessions are grouped by `project_id`; a project can be collapsed.
- **Branch**: the git branch the session works on (`branch`). Each session runs in its own worktree
  (M5), so two sessions on different branches never touch the same files.
- **Fork**: "branch the conversation". A fork copies the parent history up to `fork_seq` into a new
  session (`parent_id` = parent, `fork_seq` = last shared event) and continues independently, possibly
  with another agent/model. Forks nest under their parent and can be forked again.
- Row shows status dot (working / waiting / idle), title and agent. Selecting a row switches the chat.

**Composer.**
- Agent and model are picked with `ZSelect` (model list depends on the agent).
- `@` opens a mention palette (`ZCommand`) with three groups:
  - **Sessions**: inject a short summary of another session (never the whole transcript, token saving).
  - **Projects**: add another project path as extra context / working directory.
  - **AI**: route this message to a specific agent + model (sets the selects for this message).
- Picked mentions show as removable chips above the input; the daemon resolves them into the context
  pack when the message is sent (see section 3, "Context pack").
- Later: `@file` (fuzzy index), `/command`, `#skill`.

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
- Composer: `ZSelect` agent/model/effort; mention `@session` / `@project` / `@AI` (palette `ZCommand`, chip có thể xoá), `@file` (fuzzy từ core), `/command`, đính ảnh.
- Sidebar: cây `ZTree` Project > Branch > Session > Fork (fork lồng dưới session cha); tạo fork từ menu session; trạng thái working/waiting/idle.
- Tích hợp bộ UI của user (thay `packages/ui`).
- **DoD**: chat Claude end-to-end trong app, stream mượt, permission prompt hoạt động.

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
