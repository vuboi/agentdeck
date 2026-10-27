# Plan: Store — chợ Plugin, Skill, MCP (M10)

Mục tiêu: một tab Store giống Extensions của VS Code. Từ giao diện, user tìm, cài, gỡ, cập nhật plugin, skill, MCP và connector,
rồi gán mỗi món cho agent nào được dùng (Claude Code, Codex, Antigravity). Agentdeck không tự chạy plugin: mọi thao tác đi qua CLI
của từng agent hoặc ghi đúng folder mà agent đó đọc. Không món nào được cài trước khi qua bước kiểm tra bảo mật.

## 1. Khái niệm

| Loại | Chạy trên | Cách gán agent |
|---|---|---|
| Skill (folder có `SKILL.md`) | Mọi agent | Junction từ thư viện vào folder skills của agent |
| MCP server (stdio / http / sse) | Mọi agent | `mcp add` / `mcp remove` của từng CLI (đã có trong `mcp_manage.rs`) |
| Connector (MCP remote có OAuth) | Mọi agent hỗ trợ MCP http | Như MCP, cộng bước đăng nhập (`codex mcp login`, Claude tự mở OAuth lần đầu dùng) |
| Plugin | Chỉ agent gốc của nó | `claude plugin …` hoặc `codex plugin …`; phần skill và MCP bên trong tách ra cài được cho agent khác |

Mỗi món có ma trận tương thích: agent nào *chạy được*, agent nào *đang bật*. UI chỉ cho tick agent chạy được.

## 2. Nguồn

Mọi nguồn đổ về một kiểu `CatalogItem { id, kind, name, summary, version, source, publisher, agents_supported, review_badge }`.

1. **Chợ của CLI**
   - `claude plugin list --json --available`, `claude plugin marketplace list --json`
   - `codex plugin list --json --available`, `codex plugin marketplace list`
   - Danh bạ chính thức của Claude: `~/.claude/plugins/plugin-directory-cache-v2.json`. Đọc offline, có trạng thái review.
2. **MCP Registry**: `registry.modelcontextprotocol.io` (API JSON). Cache ở `~/.agentdeck/cache/mcp-registry.json`, chỉ tải lại khi bấm Refresh.
3. **Nguồn skill do user thêm**: repo GitHub chứa nhiều skill (ví dụ `anthropics/skills`). User thêm hoặc xoá nguồn trong tab Nguồn.
4. **Có sẵn trên máy**: skill trong folder của từng agent (đường dẫn như `slash_commands.rs` đang quét) và MCP trong config của từng agent
   (`mcp_manage.rs`). Hiện ở mục "Đã cài" với nút *Đưa vào thư viện* để gán sang agent khác.
5. **Cài ngoài store**
   - URL: repo git, file `.zip`, URL thô của `SKILL.md`, file JSON cấu hình MCP, hoặc URL endpoint MCP http.
   - Chọn folder trên máy (dialog chọn folder của Tauri).
6. **Connector**: MCP remote có OAuth (ví dụ Notion, Linear, GitHub). Lấy từ Registry (mục `remotes`) hoặc từ URL user dán.

## 3. Thư viện và cách cài

```
~/.agentdeck/
  library/
    skills/<id>/          # bản gốc của mỗi skill
    lock.json             # nguồn, commit/version, hash nội dung, kết quả quét, agent đã gán
  staging/<job-id>/       # nơi tải về và quét, chưa cài
  cache/                  # catalog đã tải
```

- **Skill**: sau khi quét đạt, chuyển từ `staging` sang `library/skills/<id>`. Tick agent nào thì tạo junction
  (`mklink /J`, không cần admin) vào folder skills của agent đó, ví dụ `~/.claude/skills/<id>` hoặc `~/.codex/skills/<id>`.
  Bỏ tick thì xoá junction. Gỡ thì xoá junction rồi xoá bản gốc. Nếu junction không tạo được (ổ không phải NTFS) thì copy và ghi `mode: copy`.
- **MCP / Connector**: gọi lại `add_args` + `run_cli` trong `mcp_manage.rs` cho từng agent được tick. Secret (env, header) nhập trên form,
  đưa thẳng vào lệnh CLI, không ghi vào `lock.json`.
- **Plugin**: `claude plugin install <p>@<m>` / `codex plugin add <p>`; gỡ, bật, tắt, cập nhật đều qua CLI. Màn chi tiết dùng `claude plugin details`
  (thành phần, số token), form option dùng `claude plugin configure --values-stdin`.
  Nút *Dùng cho agent khác*: tách các folder `skills/` và cấu hình `mcpServers` của plugin, cài như skill/MCP thường.
  Hook và command không chuyển được thì ghi rõ trên UI.
- **Cập nhật**: so commit/version với nguồn. Nội dung đổi thì quét lại (chỉ phần khác) rồi mới thay.

## 4. Kiểm tra bảo mật (bắt buộc trước mọi lần cài, kể cả cập nhật)

Chạy theo thứ tự. Mỗi bước hiện tiến độ trong dialog cài.

1. **Tải vào staging, không chạy gì**
   - Git: `git -c core.hooksPath=NUL clone --depth 1 --no-recurse-submodules`. Không chạy `npm install` hay script nào.
   - Zip: chặn zip-slip (đường dẫn `..`, tuyệt đối), giới hạn dung lượng và số file.
   - Folder local: copy vào staging, bỏ qua symlink trỏ ra ngoài.
   - MCP: chỉ lưu cấu hình (lệnh, gói npm/pypi, URL). Không chạy server.
2. **Quét tĩnh (Rust, không gọi model)**
   - Danh sách file, file nhị phân hoặc thực thi (`.exe .dll .ps1 .bat .cmd .sh .vbs`), file lớn bất thường.
   - Mẫu nguy hiểm: `curl … | sh`, `Invoke-Expression`, `powershell -enc`, `certutil -decode`, `eval`/`exec` trên chuỗi giải mã, blob base64 dài,
     đọc `~/.ssh`, `~/.aws`, `.env`, `auth.json`, sửa registry hoặc startup.
   - Plugin: liệt kê hook (`hooks.json`) và lệnh mỗi hook chạy, MCP đi kèm.
   - MCP: gói có pin version không (`@latest` là cảnh báo), tên gói gần giống gói nổi tiếng (typosquat), domain của URL.
   - `SKILL.md`: dấu hiệu prompt injection (bảo agent bỏ qua chỉ dẫn, gửi dữ liệu ra ngoài, tắt quyền xác nhận).
   - Ra `Finding { file, line, rule, severity }`, mức `info | warn | block`.
3. **Agent review (model)**: chạy một lần, headless và chỉ đọc, trong `staging/<job-id>`.
   - Claude: `claude -p --model <model> --output-format json --allowedTools Read,Glob,Grep`
     (cách gọi như `suggest_with_claude` trong `image_gen.rs`).
   - Codex: `codex exec --sandbox read-only --json`.
   - Prompt gồm manifest, kết quả quét tĩnh và schema trả về
     `{ verdict: "safe"|"caution"|"block", findings: [{file, line, severity, reason}], summary }`.
   - Nội dung đang quét có thể chứa lệnh nhắm vào chính reviewer. Prompt phải ghi rõ đó là dữ liệu, không phải chỉ dẫn.
     Model chỉ được *nâng* mức nguy hiểm, không được hạ kết quả `block` của quét tĩnh.
   - Model và agent dùng để review chọn trong Settings → Store. Mặc định là agent đã đăng nhập.
     Settings ghi rõ: nội dung món cần cài sẽ được gửi tới provider của model đó.
4. **Phán quyết**: mức cuối = max(quét tĩnh, agent).
   - `safe`: nút Cài bật.
   - `caution`: phải tick "Tôi đã xem các cảnh báo" mới cài được. Mỗi finding mở file đúng dòng bằng `openProjectFile`.
   - `block`: không cài. Chỉ cài được khi bật "Cho phép cài món bị chặn" trong Settings và gõ lại tên món.
   - Kết quả lưu theo hash nội dung trong `lock.json`. Cùng hash thì không quét lại.

## 5. Backend (Tauri, `apps/desktop/src-tauri/src/store/`)

| File | Việc |
|---|---|
| `catalog.rs` | Gộp các nguồn ở mục 2, cache, tìm kiếm |
| `fetch.rs` | Tải URL, git, zip, folder vào staging |
| `scan.rs` | Quét tĩnh, các rule ở mục 4.2 |
| `review.rs` | Agent review, parse JSON, gộp phán quyết |
| `install.rs` | Thư viện, junction, `lock.json`, gọi CLI plugin; MCP dùng lại `mcp_manage.rs` |

Lệnh Tauri: `store_catalog`, `store_refresh`, `store_installed`, `store_sources_{list,add,remove}`, `store_stage` (trả `job_id`),
`store_scan`, `store_review`, `store_install`, `store_uninstall`, `store_assign` (bật/tắt món cho một agent), `store_update`.
Tiến độ dài (clone, review) gửi qua event `store://job/<job_id>`.

## 6. Giao diện

- `StoreView.svelte` mở từ `ToolView.svelte` cạnh `McpView`. State ở `src/lib/store.svelte.ts` (Svelte 5 runes).
- Tab: **Khám phá**, **Đã cài**, **Cập nhật**, **Nguồn**. Lọc theo loại (Plugin/Skill/MCP/Connector), theo agent, theo nguồn.
  Select và danh sách dùng `PickerPopover` / `ProjectSelect`. Menu của từng món dùng `MenuButton`. Logo agent dùng `AgentLogo`.
- Card: tên, mô tả, publisher, badge review, logo các agent chạy được và đang bật.
- Màn chi tiết: README, thành phần, quyền cần dùng, ma trận agent có công tắc bật/tắt, lịch sử quét.
- Nút **Thêm từ ngoài**: dán URL hoặc chọn folder, tự nhận loại (skill/MCP/plugin), sau đó vào dialog cài.
- Dialog cài theo bước: Tải → Quét tĩnh → Agent review → Chọn agent (+ form secret cho MCP) → Cài. Có thể huỷ ở mọi bước; huỷ thì xoá staging.

## 7. Các giai đoạn

| Giai đoạn | Nội dung | DoD |
|---|---|---|
| P1 Đã cài | Liệt kê skill, MCP, plugin của cả 3 agent; bật/tắt theo agent; gỡ; đưa skill vào thư viện + junction | Thấy và quản lý đúng mọi món đang có |
| P2 Staging + quét | `fetch.rs`, `scan.rs`, `review.rs`, dialog cài theo bước | Cài từ URL và folder chỉ đi qua pipeline; test có fixture độc |
| P3 Chợ | Chợ CLI, danh bạ Claude, MCP Registry, nguồn skill GitHub; tab Khám phá và Nguồn | Cài/gỡ plugin của Claude và Codex từ UI |
| P4 Mở rộng | Plugin dùng cho agent khác, connector OAuth, tab Cập nhật | Cập nhật quét lại phần thay đổi |

Mỗi giai đoạn chạy đủ: `cargo clippy --workspace -- -D warnings`, `cargo test` (fixture, không gọi network), `pnpm svelte-check`.

## 8. Cần xác nhận

- "Connector" trong yêu cầu là MCP remote có OAuth (connector của claude.ai, app của Codex), không phải tab Settings → Connectors hiện tại (cài và đăng nhập CLI).
- Antigravity: kiểm tra `agy` có lệnh plugin không và folder skills nó đọc; chưa có thì Antigravity chỉ nhận skill và MCP.
- Định dạng JSON của `plugin list` có thể đổi theo phiên bản CLI: parser bỏ qua field lạ; CLI cũ thiếu lệnh `plugin` thì ẩn phần Plugin.
