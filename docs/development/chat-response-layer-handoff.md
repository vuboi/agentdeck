# Bàn giao: lớp phản hồi chat chung cho mọi provider (2026-10-29)

Tài liệu này để chuyển sang agent khác làm tiếp. Mọi thứ dưới đây là trạng thái thật tại thời điểm viết; chỗ nào chưa kiểm chứng đều có ghi rõ.

## 1. Mục tiêu

Phản hồi chat của AgentDeck phải cư xử như từng CLI thật (Claude, Codex, OpenCode, Antigravity, Copilot), không theo phỏng đoán. Dựng **một chuẩn chung** từ các provider đã có; provider hay connector mới sau này chỉ mở rộng và ghi đè phần khác biệt. Provider chỉ có chat thường (OpenRouter, model tuỳ chỉnh) thì AgentDeck tự bọc: điều phối, chạy ngầm, gộp phản hồi như Claude và Codex.

Yêu cầu giao diện của người dùng:
- Khi người dùng gõ nhiều tin liền lúc AI đang trả lời: việc cũ vẫn chạy tiếp, tin mới được đọc ở bước kế, mạch liền không bị cắt rồi mới gộp.
- Việc độc lập thì trả lời ngay, tin liên quan thì gộp vào việc đang chạy (cần một bước phân loại).
- Việc do agent khác làm (sub-agent, tác vụ nền): hiện tên agent (ví dụ "Haiku"), chỉ báo đang chạy có hiệu ứng sáng lướt qua, và dòng tóm tắt nhỏ kiểu "ran 1 file, used 4 tools".
- Chỉ trả lời gộp khi AI đã trả lời xong hết rồi người dùng mới gửi, hoặc khi lấy từ hàng đợi ra.

## 2. Đã làm (nằm trong working tree, CHƯA commit)

Nền: v0.1.37 đã release (nhánh `main`, commit `0ca8e5b`). Các thay đổi sau chưa commit:

| Việc | File chính | Trạng thái |
| --- | --- | --- |
| Chèn tin giữa lượt cho Claude (`inject`, `--replay-user-messages`, lệnh `agent_inject`, `injectNow`) | `crates/adapters/src/agent/warm.rs`, `claude.rs`, `apps/desktop/src-tauri/src/agent_runs.rs`, `lib.rs`, `src/lib/agent.ts`, `sessions.svelte.ts` | Test thật với Claude CLI qua code Rust PASS (1 `Done`, câu trả lời có tin chèn). Chưa chạy trong app, chưa test nhánh tin đến lúc CLI chỉ sinh chữ. Test tạm đã gỡ. |
| Claude sign-in tự mở trình duyệt (Windows) | `src-tauri/src/connectors.rs` (`pump`, `sign_in_address`) | Compile OK, chưa thử. Rủi ro hai tab nếu CLI tự mở được. |
| Antigravity: khôi phục sign-in khi huỷ (`RestoreSignIn`) và tự lưu tài khoản chưa khớp (`adopt_unsaved_agy_sign_in`) | `connectors.rs` | Compile OK, chưa thử. |
| Gọn nút đăng nhập Antigravity (bỏ bộ nút trùng) | `components/SettingsModal.svelte` | svelte-check OK, chưa xem. |
| Ctrl+Z / Ctrl+Y cho ô chat và ô prompt tạo ảnh | `src/lib/text-history.ts`, `Composer.svelte`, `ImageGenerator.svelte` | svelte-check OK, chưa thử. Sau khi gửi, Ctrl+Z ở ô trống trả lại tin vừa gửi (chủ ý). |
| Sửa Tasks treo khi đổi bộ lọc (đoạn tự nạp log dừng khi qua điểm bắt đầu của khoảng) | `components/TasksView.svelte` | svelte-check OK, nguyên nhân là suy luận, chưa tái hiện. |
| Ghi chú kế hoạch | thư mục memory của Claude Code | Có. |

`pnpm clean:junk` trước khi commit để chắc không còn file rác. `cargo clippy` và `cargo test -p ag-adapters` đã sạch (113 test pass) sau khi gỡ test tạm.

## 3. Kết quả nghiên cứu CLI (log thô và báo cáo ở `%TEMP%\probe\`)

Báo cáo: `live-claude.md`, `live-codex-opencode.md` (có "Round 2"), `live-agy-copilot.md`, `live-openrouter.md`, `history.md`. Phải chép ra chỗ khác nếu muốn giữ: thư mục Temp có thể bị dọn.

### Claude (stream-json + `--replay-user-messages`)
- Một `result` kết thúc một lượt. Tin gửi lúc đang chạy công cụ thì gộp vào lượt đó (replay xuất hiện sau `tool_result`, một `result`). Tin gửi lúc đang sinh chữ hoặc ngay sau `result` thì thành lượt mới (`system/init` mới, `result` thứ hai).
- Sub-agent (Agent tool) và lệnh nền (`run_in_background`): công cụ trả về ngay, **`result` đầu tiên đến sớm khi việc chưa xong**. Việc xong thì CLI **tự mở một lượt mới không cần tin nhắn** (`task_notification` rồi assistant rồi `result` thứ hai). Tiến trình sống đến khi việc nền xong.
- Liên kết: `parent_tool_use_id` (sub-agent), `task_id` / `tool_use_id` (nền), `message.id` (gom thinking và text của một message). Không có `isSidechain` trong stream.
- Ngắt: `control_request` interrupt, chờ `result/error_during_execution` (`aborted_tools`), coi là "đã dừng", phiên vẫn dùng tiếp.
- **Lỗi hiện có (chưa sửa):** app coi `result` đầu là hết lượt rồi đưa tiến trình về trạng thái chờ, nên lượt tự sinh sau việc nền không có ai đọc. Đây là nguyên nhân khả dĩ của hiện tượng "gửi prompt mà không thấy tin".
- Vòng 2 của Claude (3 sub-agent song song, lồng nhau, lệnh nền kèm tin giữa chừng, 4 công cụ liên tiếp): A và B đã ghi log (`live-claude-rA.jsonl`, `rB.jsonl`), chưa có báo cáo tổng hợp C, D.

### Codex (`codex app-server`)
- `turn/steer` được nhận và gộp vào cùng lượt. `turn/start` lúc đang chạy cũng gộp vào lượt đang chạy (trả cùng `turnId`); trong thử nghiệm model chỉ trả lời tin thứ hai, mất phần của tin đầu. `thread/queue/add` xếp cho lượt kế (chỉ thread không ephemeral).
- Sub-agent: `collabAgentToolCall` (`spawnAgent`, `wait`), con chạy trên `threadId` riêng; **phải lọc thông báo theo thread**, nếu không `turn/completed` của con đóng nhầm lượt cha. Một `wait` trả về khi con đầu tiên xong. Lượt cha kết thúc ở `turn/completed` của thread cha.
- Item có thể hoàn tất sau `turn/completed`. Ngắt: `turn/interrupt` cho `status: "interrupted"`, lệnh đang chạy không có `item/completed`, UI phải tự đóng. Tên con chỉ có model và thứ tự (không có tên người).
- Một lượt 4 công cụ: `fileChange` và các `commandExecution` (`exitCode`, `durationMs`, `status`).

### OpenCode
- Sự kiện: `text`, `step_start`, `reasoning`, `tool_use` (chỉ phát một lần khi đã xong), `step_finish` (có khi thiếu ở bước cuối). Hết lượt = tiến trình thoát. Kill không để lại dấu kết thúc.
- Tin thứ hai trên cùng session chạy **song song** (không xếp hàng, không gộp), hai luồng trùng `part.id` nên phải dedupe.
- Sub-agent: `tool_use` tên `subagent`, liên kết con qua `metadata.sessionID`; nhãn lấy từ `input.agent` và `input.description`.

### Antigravity (agy) và Copilot
- **Chưa có log chat thật.** Lần đầu agent kết luận sai: agy "chưa đăng nhập" (trạng thái tạm vì luồng "Sign in another account" đang xoá sign-in chung) và Copilot "chưa cài" (nó tìm gói npm; AgentDeck chạy qua crate `github-copilot-sdk`).
- Từ code và fixture (chưa kiểm chứng): agy dùng `result` làm dấu kết thúc, tin giữa lượt chưa đo; Copilot có `session.abort()` không phát `Done`, SDK có steering lane và queue (`queueSendNow` trả `steered`), sự kiện sub-agent có `agentId` và `parentToolCallId`, và `copilot.rs` hiện nối mọi `assistant.message_delta` vào câu trả lời mà không kiểm tra `agentId` (text sub-agent có thể lẫn vào).
- Agent haiku thử lại cả hai đã chạy hơn 10 phút mà chưa ghi file nào. Nếu bàn giao, chạy thẳng lệnh thay vì qua haiku.

### OpenRouter (provider tuỳ chỉnh, đã test bằng khoá free của người dùng)
- Streaming `chat/completions`: `delta.reasoning` từng token, tool call theo mảnh có `index` (song song = nhiều index), `finish_reason: tool_calls`, chunk cuối có `usage` rồi `[DONE]`. Không có sub-agent, tác vụ nền, hàng đợi hay chèn giữa lượt: chèn tin chỉ là thêm message vào lần gọi kế. AgentDeck tự chạy vòng lặp (`custom_chat.rs`, `MAX_STEPS` 40).

### Lịch sử local (`history.md`)
- Cả 5 provider đều có. Claude: `queue-operation` (có `absorbed_mid_turn`), `task-notification`, file sub-agent riêng; Codex: `turn_id`, `parent_thread_id`, nhiều file `.zst`; OpenCode: SQLite; Antigravity: `transcript.jsonl`; Copilot: `events.jsonl`.

## 4. Hợp đồng chung đề xuất

Mỗi provider khai báo năng lực, UI chỉ dựa vào năng lực:
- `midTurn`: `inject` (Claude, Codex) / `queue` / `parallel` (OpenCode).
- `turnEnd`: dấu kết thúc thật (Claude `result`, nhưng phải đợi việc nền và lượt tự sinh; Codex `turn/completed` của thread cha; OpenCode tiến trình thoát; custom tự quyết).
- `children`: cách gắn sub-agent và việc nền (`parent_tool_use_id` / `threadId` / `sessionID` / `agentId`), kèm nhãn (tên, model, mô tả).
- `grouping`: cách gom nhiều sự kiện thành một câu trả lời và một dòng "ran N tools".
- `interrupt`: cách ngắt và dấu kết thúc sau ngắt.
- Provider không có các thứ trên: lớp bọc của AgentDeck tự làm.

## 5. Việc còn lại (theo thứ tự đề xuất)

1. Thêm test cho nhánh tin đến lúc Claude chỉ sinh chữ (hai `result`, không đóng lượt sớm).
2. Sửa lỗi lượt tự sinh sau việc nền của Claude: giữ tiến trình đọc tiếp khi còn tác vụ mở (`system/background_tasks_changed`, `task_notification`), hiện lượt mới như một câu trả lời mới.
3. Đo thật Antigravity và Copilot (tin giữa lượt, ngắt, sub-agent, lẫn text sub-agent trong `copilot.rs`).
4. Viết lớp trừu tượng ở backend (trait năng lực ở `crates/adapters`), mỗi provider ghi đè phần khác biệt; xem `docs/development/adding-agent-provider.md` để nối đủ các bề mặt.
5. Bước phân loại "tin liên quan hay việc độc lập" (đã thử một bản dùng lượt phụ trả `[[RELATED]]`, bị mất khi lượt khác ghi đè; cần làm lại trên nền mới và chỉ dùng cho provider có lượt phụ).
6. UI chat: nhãn agent kèm hiệu ứng đang chạy, dòng tóm tắt "ran N tools", hiện sub-agent và việc nền dưới thẻ công cụ cha; hiện tin chèn ngay trong lượt.
7. Bọc provider chỉ có chat thường (tác vụ nền, điều phối, gộp phản hồi).
8. Thử Claude sign-in và Antigravity trong app thật; kiểm tra Tasks không còn treo khi đổi bộ lọc; thử Ctrl+Z.
9. Commit, chạy `pnpm clean:junk`, release (notes có mục "What's new" và "Fixes").

## 6. Lưu ý

- Khoá OpenRouter đã dán trong chat; người dùng nói sẽ xoá. Không lưu khoá vào repo.
- Tài khoản Codex đã đổi sang tài khoản mới; quota tuần của tài khoản cũ từng ở 93%.
- Không tự đổi tài khoản của agent; không đăng nhập hộ người dùng.
