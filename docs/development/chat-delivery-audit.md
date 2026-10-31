# Rà soát luồng chat và trạng thái composer

Ngày: 2026-10-30. Phạm vi: gửi prompt, queue, Send now, Stop/Continue, tác vụ nền và hiển thị trạng thái cho mọi model qua runtime tương ứng. Đây là kết quả rà mã nguồn và kiểm tra bộ điều phối; chưa phải chứng nhận chat thật cho mọi model/tài khoản.

## Quy tắc sử dụng

| Trạng thái | Nút và hành vi |
| --- | --- |
| Rảnh, ô nhập trống | Gửi bị vô hiệu hóa; không quay |
| Rảnh, có nội dung | Gửi ngay nếu không có queue đang chờ |
| Đang trả lời, ô nhập trống | Dừng; vòng quay thuộc lượt trả lời hiện tại |
| Đang trả lời, có nội dung | Nút xếp hàng và nút Dừng riêng, giữ bản nháp |
| Enter khi đang chạy | Thêm cuối queue; mỗi prompt có lượt trả lời riêng |
| Alt+Enter khi đang chạy | Đưa prompt lên đầu queue, giữ lượt hiện tại |
| Ctrl/Cmd+Enter hoặc Send now | Cùng một chính sách ưu tiên; mở lượt độc lập nếu có chỗ an toàn, hoặc đưa lên đầu queue. Không tự hủy công việc cũ |
| Queue tạm dừng | Không tự lấy prompt kế; bấm Gửi hoặc Send now thì tiếp tục trực tiếp, không mở menu lựa chọn |
| Đang chờ câu trả lời/duyệt quyền | Queue chờ; Send now trên từng dòng không bỏ qua yêu cầu này |
| Chỉ còn tác vụ nền sau khi lượt đã kết thúc | Thẻ nền còn hiển thị; nút Gửi không quay vì tác vụ nền |
| Sau Stop | Queue đang có được giữ và tạm dừng; Continue tiếp tục công việc bằng một lượt mới |

Pause chỉ tác động queue. Stop kết thúc lượt theo cơ chế của provider; Continue không hồi sinh tiến trình bị hủy. Hiệu ứng Markdown và vị trí cuộn không quyết định khi nào provider được nhận prompt kế.

## Đối chiếu runtime

| Provider | Đường chạy hiện tại | Kết thúc/ngắt và công việc nền |
| --- | --- | --- |
| Claude | CLI stream-json được giữ ấm | Result kết thúc lượt; interrupt giữ phiên sử dụng lại; tail có thể phát auto-turn khi công việc nền báo về |
| Codex | App-server qua adapter | Lượt kết thúc theo thread/turn; interrupt; sự kiện con được tách phạm vi |
| Antigravity | CLI stream-json qua adapter | Result kết thúc lượt; queue của app giữ liên kết prompt/phản hồi; trạng thái nền phụ thuộc sự kiện provider |
| OpenCode | Server API trong runtime desktop | Có đường interrupt và luồng sự kiện riêng; runtime dùng hồ sơ bảo thủ, chưa tích hợp đầy đủ steer, tác vụ nền và duyệt quyền |
| Copilot | SDK, client được giữ theo chat | Session idle kết thúc lượt; abort/drain; có thông báo nền và sự kiện con |
| Custom, kể cả model qua OpenAI-compatible endpoint | Vòng gọi model và tools của AgentDeck | Queue của app; hủy run; không mặc định có cơ chế auto-turn hay chạy nền như Claude |

Model và connector tài khoản cung cấp cấu hình/credential cho runtime. Việc dùng chung UI không bổ sung khả năng native mà provider chưa có. Không chèn prompt vào cùng lượt khi provider không bảo đảm được liên kết riêng với phản hồi.

## Lỗi đã sửa

- Send now từ queue và từ composer dùng chung chính sách. Click lặp/stale trên queue không gửi trùng; prompt còn lại được giữ. Gửi và Send now bỏ trạng thái Pause để tiếp tục trực tiếp.
- Stop ở foreground listener cũng tạm dừng queue trước khi trả về.
- Run bị dừng trong lúc lưu ảnh, đọc slash command hoặc đọc project memory không được khởi chạy muộn. Custom provider kiểm tra lại sau khi đọc dữ liệu ảnh.
- Các prompt cùng thức dậy sau đổi credential không bị bỏ khi prompt đầu tiên đã chiếm lượt; chúng được giữ theo thứ tự trong queue.
- Queue không còn chờ tối đa 15 giây vì một block Markdown nằm ngoài virtual viewport.
- Request, tool và thinking trong Activity được phân theo turn: một lượt phụ kết thúc không đóng nhầm công việc của lượt khác.
- Result thành công không tự đánh dấu tác vụ nền đã dừng. Task event đến ở lượt sau cập nhật thẻ gốc và trạng thái Activity; progress đến muộn không mở lại thẻ đã hoàn tất.
- Copilot và OpenCode xóa run khỏi registry backend sau khi hoàn tất.
- Composer dùng replies có phản ứng; icon queue, trạng thái Pause và vòng quay phản ánh hành vi gửi thực tế. Có nút Dừng khi đang gõ.

## Bằng chứng và giới hạn

- 62/62 kiểm tra JavaScript qua: reducer reply, queue, lanes, run state và bộ điều phối thực trong VM với native I/O giả lập. Có ma trận sáu provider cho Send now, pending input, credential FIFO và hủy trong lúc chuẩn bị. Các test này không gọi mạng và không chứng minh đăng nhập/chat thật.
- Svelte check: 0 lỗi, 0 cảnh báo. Build Vite thành công; có cảnh báo CSS highlight, chia chunk/import động và bundle lớn.
- TypeScript check của hai package UI/protocol thành công. Lệnh pnpm tổng bị shim/corepack của môi trường chặn, nên chạy trực tiếp các entry Node đang cài.
- Cargo check desktop và workspace thành công. Clippy workspace bị chặn bởi `type_complexity` trong kiểu engine tìm kiếm của custom provider, ngoài phần thay đổi luồng chat.
- Cargo test workspace với Git shell trên PATH: adapter 138/138 qua; desktop 181 qua, 5 lỗi, 1 bỏ qua. Năm lỗi thuộc parser Bing (khoảng trắng), nội dung hướng dẫn delegate, hai test server bị Access denied và index SQLite readonly. Test gọi provider thật vẫn được bỏ qua theo cấu hình sẵn.
- Preview mở bằng `pnpm dev:web`; dùng fixture trong bộ nhớ. Đã bấm Pause, gửi trực tiếp khi queue dừng, xác nhận giữ bản nháp/nút Stop và kiểm tra chỉ có tác vụ nền thì thẻ còn hiện nhưng nút Gửi không quay.
- Preview gặp ACL Tauri của app release. Chưa chạy lại ma trận native có đăng nhập cho từng provider; chưa xác nhận mọi model/connector đều thành công. Kiểm tra responsive giới hạn ở viewport desktop của Preview; không mở rộng sang thiết kế mobile.

## Chạy lại kiểm tra hồi quy

### Bổ sung: thanh công việc và kết quả nền

- Thanh Work nằm cố định ngoài virtual transcript. Nó đọc sự kiện Activity, task của delegate_task và worker của workflow; không phụ thuộc các thẻ tin nhắn đang được tải.
- Cùng một thanh hiển thị lệnh nền, sub-agent native, worker cùng provider và worker qua connector khác. Mỗi dòng có tên việc, provider/model, trạng thái, thời gian và chi tiết. Có nút mở worker, dừng riêng task khi runtime hỗ trợ và trả lại kết quả đã lưu nếu bước tổng hợp bị ngắt.
- Việc hoàn tất còn trong danh sách kết quả. Cuộn chat, đổi chat và nạp lại transcript không làm mất work record. Snapshot được lưu định kỳ; sau reload, việc chưa được xác nhận lại hiển thị Check status. Đây không phải cơ chế daemon tự phục hồi mọi worker.
- Send now ưu tiên không hủy. Một lane phụ chỉ mở khi provider đang thực sự gọi công cụ chờ và đã có công việc nền; việc tạo sub-agent trong lúc agent chính còn sinh nội dung không tự coi là rảnh.
- Báo cáo worker giữ sourceMessageId và nội dung yêu cầu gốc. Các báo cáo khác yêu cầu không bị gộp; không lấy prompt gần nhất. Báo cáo có lượt trả lời riêng và nhãn Task result. Báo cáo native đến khi foreground đang chạy cũng có reply riêng.
- Hàng báo cáo được đánh thức bằng sự kiện hoàn tất, không polling một giờ rồi xóa kết quả. Prompt đang chờ và yêu cầu người dùng vẫn có thứ tự rõ ràng.
- Các kiểm tra hồi quy mới bao phủ work projection, khử trùng lặp workflow, ưu tiên không hủy, nguồn báo cáo, listener sau nạp transcript và auto-turn đồng thời với foreground. Chưa thực hiện ma trận chat thật cho mọi tài khoản/provider.

### Luồng Stop độc lập của Work

1. `trackActivity` đổi `browser_serve` thành Activity `Run project`, giữ `activityKey` và thông tin command/folder của server do AgentDeck sở hữu.
2. `projectChatWork` chiếu Activity thành row Work. Chỉ `Run project` đang sống được đặt `canStop: true`; shell native khác không có nút Stop riêng.
3. `stopTarget` chọn đúng một đích thuộc row: `project`, `delegate` hoặc `child`. Nếu row không có handle độc lập, kết quả là `null`; không có fallback về `sessionId` của main chat.
4. `BackgroundTasks` gọi `stopProjectRun` cho `project`, `stopDelegateTask` cho worker delegated và `stopTurn(childId)` cho workflow child.
5. `stopProjectRun` gọi `server_stop` đúng server key. Sau khi lệnh thành công, Activity được ghi `stopped`, có `endedAt`, audit được cập nhật và row rời nhóm `active`.
6. Work tray gọi `refreshProjectRun` mỗi giây. Nếu server bị kill ngoài app, `servers_list` trả `running: false` hoặc không còn row; Activity cũng được chuyển sang `stopped`.
7. Stop của composer/main chat vẫn đi qua `stopTurn(sessionId)` và là luồng riêng; Stop một `Run project` không gọi hàm này.
8. Nút global của Work là `Stop all` (không phải pause): nó lặp qua từng `stopTarget` độc lập, không gọi `stopTurn(sessionId)` và không làm ảnh hưởng lượt main ngoài các row được người dùng chọn dừng hàng loạt.

Regression cases:

- `chat-work.test.ts`: project run có Stop độc lập; native shell không có Stop; delegate và child có đúng target; row không có handle không được suy diễn thành Stop main.
- `svelte-check`: kiểm tra wiring của Work tray, `AgentActivityCards` và các nhánh Stop.
- Còn cần kiểm tra desktop thật với Tauri: bấm Stop trên row `Run project`, xác nhận server tắt, main model vẫn sinh tiếp, rồi force-kill ngoài shell và xác nhận row tự chuyển `Stopped`.

```powershell
node --experimental-strip-types --experimental-vm-modules --test apps/desktop/tests/chat-delivery.test.mjs apps/desktop/src/lib/chat/agent-reply.test.ts apps/desktop/src/lib/chat/runs/queue.test.ts apps/desktop/src/lib/chat/runs/run-lanes.test.ts apps/desktop/src/lib/chat/runs/run-state.test.ts
pnpm --filter desktop check
pnpm --filter desktop build
cargo check --workspace
cargo clippy --workspace -- -D warnings
cargo test --workspace
```

Trên Windows, cần Node thật thay cho mise shim hỏng, Rust toolchain ổn định và Git shell trên PATH khi chạy các fixture tiến trình.
