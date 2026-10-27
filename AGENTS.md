# AGENTS.md — quy ước cho mọi coding agent

Đọc `PLAN.md` trước. Chỉ làm đúng module được giao, không làm trước module sau.

## Môi trường (Windows)
- Rust: dùng `~/.cargo/bin` (shim mise cho cargo bị lỗi). Trong bash: `export PATH="$HOME/.cargo/bin:$PATH"`.
- Node 24, pnpm 10. Không dùng npm/yarn để cài.

## Rust
- Edition 2021, workspace dependencies khai báo ở root `Cargo.toml` (`[workspace.dependencies]`).
- Lỗi: `thiserror` trong lib, `anyhow` trong bin. Không `unwrap()` ngoài test.
- Async: `tokio`. Log: `tracing`.
- `cargo clippy --workspace -- -D warnings` phải sạch.

## Frontend
- Svelte 5 runes (`$state`, `$derived`, `$effect`, `$props`). Không dùng store API cũ cho code mới.
- TypeScript strict. `svelte-check` không lỗi.
- Không thêm thư viện UI lớn; `packages/ui` sẽ được thay bằng bộ UI của user.

## Chung
- Không commit secret. Không gọi network trong test.
- Giữ thay đổi tối thiểu, đúng phạm vi module.
- Kết thúc: chạy toàn bộ lệnh DoD của module và báo kết quả thật.

## Popover và danh sách chọn (UI)
- Mọi popover/menu chọn dùng `ZPopover` (hoặc `PickerPopover` / `ProjectSelect` đã dựng sẵn), không tự đặt `position: absolute` ở một góc riêng.
  Popover neo đúng element mở nó (`anchor`), đặt `zPosition` theo chỗ trống, và độ rộng theo nội dung (một danh sách ngắn thì hẹp, không rộng cả panel).
- Element trigger luôn sáng màu khi popover đang mở: dùng `TRIGGER_ON` / `TRIGGER_ON_PLAIN` trong `src/lib/popover.ts` (giống nút chọn model của ô chat).
- Danh sách trong popover hover bằng `zHoverGlide`: `{@attach glide()}` trên danh sách, `data-glide-item` trên từng dòng (`glide` ở `src/lib/popover.ts`).
  Dòng đang chọn / đang active có nền riêng `ROW_ON` (`bg-primary/15 text-primary`) và đặt `aria-selected="true"` để không nhận hover box.
- Popover có ô tìm kiếm tự focus vào ô đó khi mở. Danh sách dài dùng virtual scroll và mờ hai đầu (xem `PickerPopover`).
- Mỗi hàng có icon: agent dùng logo thương hiệu (`AgentLogo`, truyền `agent`), không dùng icon chung.
- `ZCommand` chỉ còn cho palette tìm chat (`SearchPalette`); chỗ khác dùng popover.
- Hover không được ghi đè trạng thái active: tab đang mở, nút đang bật, tool đang chọn giữ màu `bg-primary/15 text-primary` khi rê chuột
  (dùng `ACTIVE_ON` / `TRIGGER_ON` trong `src/lib/popover.ts`, có `!` để thắng hover riêng của button).

## Component dùng chung (không tự dựng lại từng chỗ)
- Danh sách chọn, ô tìm kiếm, danh sách tab: `PickerPopover` (cỡ `sm`/`md`, `search`, `local`, `headings`).
- Menu chuột phải / menu của nút: `Menu` (mở tại điểm bằng `at`, hoặc trên `anchor`) và `MenuButton` (nút icon có menu). Truyền mảng item
  như `ZDropdownMenuItem`; không dùng `ZDropdownMenu` nữa.
- Select lọc theo project: `ProjectSelect`. Danh sách tab có tìm kiếm: `TabListButton`.
- Mở file từ đường dẫn bất kỳ: `openProjectFile` (`file-viewer.svelte.ts`) với các hàm trong `src/lib/paths.ts`; không tự cắt/ghép đường dẫn.

## Hai popover model (chat input và ô tạo ảnh AI): luôn sửa đồng bộ
- `ModelPicker.svelte` (chat) và `ImageGenerator.svelte` (gen ảnh) dựng cùng kiểu popover: danh sách nested, Fast mode, effort, divider
  Free / Hide / Paid. Sửa giao diện hay thứ tự ở một bên thì sửa luôn bên kia trong cùng lượt (cỡ công tắc `zSize="sm"`, divider,
  scrollbar, thứ tự model). Thứ tự và divider model dùng chung `freeFirst` trong `src/lib/models.svelte.ts`: đừng tự xếp lại ở từng chỗ.
