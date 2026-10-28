# Quy trình thêm agent/provider mới

Tài liệu checklist cho agent code tiếp theo. Đọc `AGENTS.md` và `PLAN.md` trước khi sửa. Dùng GitHub Copilot hiện tại làm ví dụ về provider SDK có đăng nhập qua một CLI khác; đừng sao chép nguyên cách auth/MCP của nó nếu provider mới có SDK hoặc CLI riêng.

## Nguyên tắc

- Chốt một provider ID ổn định, viết thường, ví dụ `copilot`. Dùng cùng ID xuyên suốt Rust, Svelte, lưu session, workflow, MCP và skills.
- Tìm và mở rộng registry/type/list đang có; không tạo picker, logo, model cache, connector hay skill manager thứ hai.
- Mỗi provider có thể cần nhiều lớp riêng: đăng nhập, account switch, runtime, model discovery. Đừng coi việc thêm card Settings là tích hợp xong.
- Provider chat không tự động là provider tạo ảnh. Phân biệt model dùng để soạn prompt với backend thực sự tạo ảnh.
- Giữ thay đổi của người dùng đang có trong workspace; không reset hoặc format đệ quy các file ngoài phạm vi.
- Mỗi bề mặt dưới đây thường có một chuỗi `if agent === ...` hoặc danh sách agent viết tay riêng. Tìm theo ID mới (`grep -rn "'claude'" apps/desktop/src`) để không sót chỗ nào.
- Trước khi viết code gọi API, tra cách client chính thức của provider làm (repo mã nguồn mở của vendor trên GitHub). Dùng đúng endpoint và header mà client đó dùng; không giả header của client khác.

## Bẫy đã gặp (Copilot)

- **Gửi chat bị chặn:** `sessions.svelte.ts` có danh sách agent được gọi `startTurn`. Provider không có trong danh sách sẽ rơi vào nhánh "`<agent> chats are not connected yet`" dù backend đã có `agent_send`. Thêm ID mới vào đúng danh sách này.
- **Quota bar không hiện:** `Composer.svelte` (`const quota = $derived.by`) chỉ tính cho agent có nhánh riêng. Thêm nhánh đọc `connectors.accounts[<id>]` (cửa sổ `usage`, `observed_at`) và để `accounts_list` của provider đó điền `usage` cho account active.
- **Quota không tự làm mới:** vòng kiểm tra 5 phút trong `connectors.svelte.ts` (`setInterval(check, 5 * 60_000)`) chỉ refresh provider được liệt kê. Thêm provider nếu cần cảnh báo ngưỡng.
- **Đơn vị cửa sổ:** `UsageWindow.pct` là phần đã dùng (%), còn quota bar hiển thị phần còn lại (`100 - pct`). Nhãn cửa sổ phải khớp regex `^\d+(m|h|d)$` (ví dụ `30d`) để tính cửa sổ "gần hết nhất".
- **Lệnh CLI bị chặn:** `account_tool_allows` (`connectors.rs`) chỉ cho phép một số lệnh của `codex-auth`/`cswap`. Gọi CLI khác (ví dụ `gh api`) không bị chặn, nhưng cần xác nhận khi thêm công cụ mới.

## Checklist theo bề mặt

### 1. ID, logo và thông tin agent

- Thêm profile vào `apps/desktop/src/lib/provider-registry.ts` (`BuiltInAgent`, `BUILT_IN_AGENTS`). Đây là nguồn chung cho logo, tên, icon và màu trên chat, workflow và picker.
- Thêm agent vào các union/type còn độc lập: `sessions.svelte.ts`, `mock.ts`, slash-command types và connector types khi cần.
- Rà các bảng màu viết tay ở `Chat.svelte`, `Sidebar.svelte`, `SettingsModal.svelte` và MCP view. Ưu tiên `AgentLogo`/registry thay vì thêm icon chung.
- Session của AgentDeck lưu provider trên từng chat; kiểm tra nhãn và logo trong danh sách chat gần đây, header, tab và task card. Không nhầm session mới của AgentDeck với history import của CLI.

### 2. Đăng nhập, account và Settings

- Backend connector ở `apps/desktop/src-tauri/src/connectors.rs`: phát hiện binary, `Login`, trạng thái đăng nhập, login/cancel, account list/add/switch và thông báo lỗi dễ hiểu.
- Frontend ở `apps/desktop/src/lib/connectors.svelte.ts`: cập nhật `ConnectorId`, `AccountProvider`, `Detected`, state, refresh và thao tác switch. Không đưa token hoặc output có token vào UI/log.
- Thêm connector card trong `components/SettingsModal.svelte`, gồm binary, logo, mô tả, login và account manager nếu provider hỗ trợ nhiều account.
- Khi account đổi, gọi `accountSwitched(providerId)` để chat không resume bằng credential cũ; tải lại model list nếu catalog phụ thuộc account.
- Nếu CLI là cách đăng nhập được chọn, chỉ đọc credential từ CLI tại thời điểm cần, truyền thẳng vào SDK và không lưu token vào preferences, source hoặc log.

### 3. Runtime chat, session và model discovery

- Runtime SDK không đi qua `Provider` CLI/stdout nếu giao thức khác. Thêm nhánh riêng tại `apps/desktop/src-tauri/src/agent_runs.rs` trước `provider_for`, dùng cùng `TurnRequest`, stop tracking và event envelope của AgentDeck.
- Dịch event SDK thành `AgentEvent` hiện có: delta, thinking, tool call/result, usage, permission, done/error. Dọn session/client khi kết thúc, lỗi hoặc cancel.
- Nếu SDK/provider có diff trong arguments, map vào `AgentEvent::ToolCall.change` (`FileChange`: path, +/- counts, before/after) để wrapper hiện ngay khi tool bắt đầu.
- Nếu provider chỉ gửi đường dẫn, adapter có thể chụp file trước tool và gửi diff ở `AgentEvent::ToolResult.change` sau khi hoàn tất. Dùng chung `begin_file_snapshot` / `finish_file_snapshot`: chỉ đọc file text nhỏ nằm trong project root; file binary, file lớn hoặc ngoài root thì bỏ qua. Không lấy Git workspace snapshot làm fallback vì sẽ trộn thay đổi có trước lượt chat.
- Chat tự gom `FileChange` trong message thành wrapper `Edited N files, +X −Y` và preview. Giữ các block cùng message để AgentDeck lưu được cùng lịch sử chat; không tạo panel hay kho lưu riêng cho provider.
- Dùng ID session riêng của provider cho resume; không tạo lại session mỗi lượt. Kiểm tra account switch có bắt đầu session mới.
- Nối `models_list` trong `src-tauri/src/lib.rs` tới model discovery của provider. Trong `models.svelte.ts`, cập nhật `ProviderId`, catalog, source, `modelsReady` và `loadModels`.
- Tải lại model sau login/switch. Nếu provider chưa đăng nhập hoặc không lấy được catalog, trả lỗi/danh sách rỗng có kiểm soát, không làm hỏng picker của provider khác.
- Workflow lấy model từ `modelSources()` (`components/flow/FlowEditor.svelte`); kiểm tra workflow có thể chọn, lưu và chạy provider mới.

### 4. Chat input, file/ảnh và Image Generator

- Chat input gọi `agent_send`; kiểm tra model, effort, permission mode, allowed approvals, stop/cancel, file context và session resume đều tới đúng runtime.
- Nếu composer đang encode ảnh/tệp trong prompt bằng đường dẫn, chuyển các đường dẫn đó thành attachment native của SDK khi có API hỗ trợ; giữ nguyên text/context hiển thị cho lịch sử.
- `ModelPicker.svelte` dùng `modelSources()`; kiểm tra provider xuất hiện ở composer và `@AI` routing.
- `ImageGenerator.svelte` có model để **soạn prompt** và một backend riêng để **tạo ảnh**. Provider chat mới có thể là prompt model nếu tương thích; chỉ thêm thành image-generation backend khi đã xác minh API tạo ảnh thật.
- Nếu thay cấu trúc hoặc UI picker model, đồng bộ `ModelPicker.svelte` và `ImageGenerator.svelte` theo quy ước trong `AGENTS.md`.

### 5. MCP và Store

- Cập nhật `McpAgent` trong `src/lib/mcp.svelte.ts`, danh sách provider trong `components/McpView.svelte`, và whitelist `AGENTS` trong `src-tauri/src/mcp_manage.rs`.
- Nếu provider dùng SDK, chuyển MCP chung vào cấu hình session SDK; thêm đọc/ghi config riêng khi Store cần bật/tắt server theo provider. Bảo toàn các entry khác khi cập nhật config và không ghi đè file lỗi định dạng.
- Xác nhận `servers_for(project)` chỉ trả MCP phù hợp với project/provider và SDK nhận đúng schema stdio/http, env, headers.
- Rà MCP browser nội bộ của AgentDeck: nếu provider cần dùng browser tools thì truyền bridge vào session và cập nhật danh sách agent tương ứng; đừng giả định MCP user config tự có bridge.
- Kiểm tra add, list/merge, test, remove và trạng thái disabled. Cập nhật mô tả UI nếu provider lưu config theo cách khác CLI `mcp add`.

### 6. Skills và Store

- Thêm provider vào `store/library.rs::SKILL_AGENTS` và `skill_folders`; đây là chỗ Store gán/bỏ gán skill vào folder agent.
- Đồng bộ danh sách ở `components/StoreView.svelte`.
- Thêm đọc skills vào `slash_commands.rs` và allowed roots của `slash_commands::read`, để `/skill` nhìn thấy đúng skill và không mở file ngoài thư mục cho phép.
- Bật skill discovery của SDK/runtime và truyền folder nếu provider cần cấu hình tường minh. Kiểm tra folder user, folder project và đường dẫn junction do Store tạo.
- Cập nhật bộ lọc provider trong Store Discover nếu catalog có nội dung dành riêng cho provider.

## Copilot hiện tại — ví dụ tích hợp SDK

- Profile/logo: `provider-registry.ts`, thương hiệu `GITHUB_SVG` trong `brand-icons.ts`.
- Auth/account: Settings gọi GitHub CLI; token active lấy bằng `gh auth token`, switch bằng `gh auth switch`. SDK nhận token trong bộ nhớ.
- Runtime/model/chat: `src-tauri/src/copilot.rs`; nhánh runtime tại `agent_runs.rs`; model listing tại `lib.rs`.
- Attachments: các dòng `Attached image(s)` và `Attached file(s)` từ composer được đổi thành SDK file attachments. Đây là ảnh đầu vào cho chat, không phải API tạo ảnh.
- MCP: Store lưu cấu hình Copilot tại `~/.copilot/mcp-config.json`; Rust chuyển cấu hình chung vào SDK session.
- Skills: Store gán vào `~/.copilot/skills`; slash palette đọc skills provider và `.github/skills` của project.
- Không thêm Copilot vào cảnh báo quota Claude/Codex hoặc image backend khi Copilot không cung cấp dữ liệu/tính năng tương ứng.

## Kiểm tra trước khi bàn giao

- `cargo check -p agentdeck-desktop`
- `cargo clippy -p agentdeck-desktop -- -D warnings`
- `pnpm --filter desktop check`
- Chạy app thật (`pnpm dev` hoặc tải lại cửa sổ sau khi HMR không nạp hết): đăng nhập, mở model picker, gửi một tin nhắn ngắn, kiểm tra trả lời và quota bar. Build xanh không chứng minh chat chạy được.
- Nếu người dùng yêu cầu kiểm thử, thêm/chạy kiểm tra phù hợp; không gọi network trong test. Runtime đăng nhập/chat thật cần xác minh riêng với tài khoản có quyền dùng provider.
- Báo rõ bề mặt nào đã nối, lệnh kiểm tra thực sự chạy, và giới hạn runtime chưa thể xác minh. Không tuyên bố login/chat thành công nếu chưa chạy lượt thật.
