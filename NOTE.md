# NOTE: menu chuột phải bị trôi khi scroll (agentdeck + z-ui)

## Triệu chứng
- Menu (sidebar "...", chuột phải session/file/tab/link/network) mở đúng chỗ click nhưng khi list scroll thì menu đứng yên, row trôi đi.

## Nguyên nhân
- Hiện tại menu neo vào một marker \ixed z-50 size-px\ đặt tại toạ độ click (\style:left/top\).
- Toạ độ fixed theo viewport, không theo row. Row nằm trong list đang cuộn nên dịch chuyển, marker không.
- Các menu bị ảnh hưởng: Sidebar, FileExplorer, Chat (tab menu), LinkMenu, NetworkPanel.

## Cách demo z-ui làm (đúng)
- ZPopover/ZDropdownMenu bám vào **element trigger** qua \utoUpdate\: scroll ancestor thì tự tính lại vị trí.
- Trigger là chính element (nút "..." hoặc row), không phải marker tạm.

## Hướng xử lý (chưa làm)
1. Sidebar: mỗi row bọc \ZDropdownMenu\ với custom trigger là nút "..." (hoặc row). Menu bám row, scroll theo.
2. Chuột phải: giữ \ind:open\ điều khiển bằng tay, đặt trigger = row, \zTrigger\ manual để click không mở menu.
3. Lặp lại cho FileExplorer, tab Chat, LinkMenu, NetworkPanel.
4. Nếu chưa kịp: dùng \zScrollClose\ hoặc đóng menu ở handler scroll của list.

## Lưu ý đã biết
- Thử \zPoint\ (virtual reference) trong z-ui: menu bị \isibility: hidden\, vị trí sai (\left 98 top 633\). Đã gỡ khỏi z-ui, không dùng.
- z-ui 0.1.11 đã publish (tooltip tắt sớm). agentdeck đang dùng 0.1.10 từ registry tới khi 0.1.11 hiện trên npm; sau đó chạy \pnpm install\ trong agentdeck.
- Menu mở ngay dưới nút "..." sau khi sửa anchor (đã kiểm tra trong mock).
