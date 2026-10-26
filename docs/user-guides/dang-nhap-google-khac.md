# Đăng nhập thêm tài khoản Google (Antigravity)

AgentDeck giữ nhiều tài khoản Google cho Antigravity (`agy`) và đổi qua lại mà không phải đăng xuất CLI. Khi bạn thêm tài khoản, AgentDeck lưu token của tài khoản đang dùng trước, xoá phiên cũ, rồi mở `agy` trong một cửa sổ console để nó chạy đăng nhập Google.

> Chỉ chạy trên **Windows** (token nằm trong Windows Credential Manager) và cần đã cài `agy`. Giao diện app là tiếng Anh; tên nút dưới đây giữ nguyên như trên màn hình.

## Đường nhanh

```
Ctrl+,  →  Connectors  →  thẻ Antigravity  →  Sign in another account
```

## Vị trí các nút

**Settings › Connectors** (khi đã có ít nhất một tài khoản):

```
┌────────────────────────────────────────────────────────────────────────────┐
│ Settings                                                      [Refresh] [X]│
│ ┌──────────────┐  Providers …                                              │
│ │ Connectors > │  ──────────────────────────────────────                   │
│ │ Providers    │  Antigravity  [Signed in] 0.1.x  [Sign in another…]       │
│ │ Limit alerts │  Account: ban@gmail.com · Pro                             │
│ │ Sync         │  ── ANTIGRAVITY ACCOUNTS ─────  [Retry save]              │
│ │ Computer use │  ban@gmail.com                   IN USE                   │
│ │              │  viec@gmail.com                       [⇄]                 │
│ └──────────────┘  Usage …                                                  │
└────────────────────────────────────────────────────────────────────────────┘
```

Hai nút ở tiêu đề **Antigravity accounts** nằm cùng hàng, sát bên phải: **Retry save** (chỉ hiện sau khi lưu tài khoản thất bại) rồi **Sign in another account**. Nút **⇄** ở cuối mỗi hàng tài khoản, chỉ hiện ở hàng **không** phải tài khoản đang dùng.

Khi **chưa đăng nhập tài khoản nào**, nút **Sign in** nằm ở góc phải trên của thẻ Antigravity, và nút **Sign in another account** chưa hiện.

**Thanh dưới ô nhập tin nhắn** (chat đang dùng agent Antigravity):

```
[ project ▾ ]  [ branch ▾ ]                    [ ⇄ ban@gmail.com ▾ ]
                                               ↑ chip tài khoản, sát phải
```

| Nút | Vị trí | Dùng khi |
| --- | --- | --- |
| **Settings** (`Ctrl+,`) | icon bánh răng ở cuối thanh dưới của sidebar trái, ngoài cùng bên phải (sau nút Update) | mở Settings |
| **Connectors** | mục đầu tiên, cột trái Settings | xem CLI và tài khoản |
| **Sign in** | góc phải trên thẻ Antigravity (thẻ thứ 3 trong Connectors) | chưa đăng nhập tài khoản nào |
| **Cancel** | thay chỗ **Sign in** khi đang chờ đăng nhập | bỏ lượt đăng nhập đang chờ |
| **Sign in another account** | hàng “Antigravity accounts”, sát phải | đã có 1 tài khoản, thêm tài khoản Google khác |
| **Retry save** | cùng hàng, bên trái **Sign in another account** | hiện khi tài khoản vừa đăng nhập chưa lưu được |
| **Refresh** | góc phải trên Settings, bên trái nút **X** (chỉ ở mục Connectors) | đọc lại CLI và danh sách tài khoản |
| **⇄** (Use this account) | cuối hàng tài khoản trong Settings | đổi tài khoản trong Settings |
| **chip email ▾** | cuối thanh dưới ô nhập tin nhắn | mở nhanh danh sách tài khoản |
| **+** (Add account) | góc phải tiêu đề popover của chip | thêm tài khoản từ popover |
| **Open sign-in** → **Continue and switch** | hộp thoại “Sign in to Antigravity” | token của tài khoản đã hết hạn |
| **Switch account** | hộp thoại “Switch account?” | xác nhận đổi tài khoản |

Nhãn trên hàng tài khoản: **IN USE** = đang dùng, **Needs sign-in** = credential đã lưu hỏng hoặc hết hạn. Trong popover, mỗi hàng có tag **In use / Switch / Limit / Sign in**; hàng **Limit** bị mờ và không bấm được.

## Các bước thêm tài khoản Google khác

1. Mở **Settings** (`Ctrl+,`) → **Connectors**. Thẻ **Antigravity** phải đang **Signed in**.
2. Bấm **Sign in another account** (hàng “Antigravity accounts”).
3. AgentDeck lưu token tài khoản hiện tại, xoá phiên cũ (`~/.gemini/jetski-standalone-oauth-token` và credential `gemini:antigravity`), rồi mở **cửa sổ console Antigravity CLI**. Cửa sổ đó tự mở **trang đăng nhập Google** trong trình duyệt.
4. Trên trang Google:
   - chọn tài khoản muốn thêm nếu email đã có trong danh sách;
   - nếu chưa, bấm **Use another account** (Dùng tài khoản khác) ở cuối danh sách → nhập email → **Next** → mật khẩu → **Next**;
   - đồng ý quyền nếu Google hỏi.
5. Xong, quay lại AgentDeck. Tài khoản được lưu tự động, xuất hiện trong “Antigravity accounts” và mang nhãn **IN USE**. Đóng cửa sổ console là được.
6. Nếu danh sách chưa đổi, bấm **Refresh** ở góc phải trên Settings.

## Đổi giữa các tài khoản

- **Nhanh:** bấm **chip email** ở cuối thanh dưới ô nhập tin nhắn → chọn tài khoản → hộp thoại **Switch account?** → **Switch account**.
- **Trong Settings:** hàng tài khoản → nút **⇄** ở cuối hàng.

AgentDeck hỏi Google quota của tài khoản đó trước, rồi mới ghi token sang credential `gemini:antigravity` mà `agy` đang đọc; nhãn **IN USE** chuyển theo. Lần chạy `agy` tiếp theo dùng tài khoản mới.

## Kiểm tra nhanh (PowerShell)

```powershell
# Danh sách tài khoản đã lưu: chỉ email và trạng thái, không có token
Get-Content "$HOME\.agentdeck\antigravity-accounts.json" -Raw

# Token của từng tài khoản nằm trong Credential Manager (tên đã mã hoá email)
cmdkey /list | Select-String "AgentDeck Antigravity"
```

## Khi trục trặc

| Hiện tượng | Cách xử lý |
| --- | --- |
| Tài khoản có nhãn **Needs sign-in** | Credential đã lưu thiếu/hỏng/hết hạn: chip tài khoản → dòng đó (tag **Sign in**) → **Open sign-in** → đăng nhập đúng email đó → **Continue and switch** |
| Báo “the Antigravity sign-in has expired: run agy once to renew it” | Token đã quá hạn: chạy `agy` một lần để nó tự gia hạn, rồi chuyển lại; hoặc đăng nhập lại tài khoản đó |
| Báo “saved Antigravity credentials are missing” | Credential của tài khoản đã bị xoá khỏi Windows: đăng nhập lại rồi lưu |
| “Sign-in completed, but the account was not saved” | Email vừa đăng nhập không khớp hàng tài khoản, hoặc lưu lỗi: đóng rồi mở lại hộp thoại, hoặc bấm **Retry save** trong Settings |
| “Antigravity CLI exited before sign-in completed” | Cửa sổ console bị đóng trước khi xong: chạy lại **Sign in another account** |
| Đăng nhập quá 5 phút không xong | AgentDeck dừng chờ: chạy lại **Sign in another account** |
| Không thấy **Sign in another account** | Chưa đăng nhập tài khoản nào (dùng **Sign in** ở thẻ), hoặc `agy` chưa cài (bấm **Install**) |
| Không thấy chip tài khoản dưới ô chat | Chat chưa chọn agent Antigravity, hoặc `agy` chưa cài |
| Báo lỗi Credential Manager trên macOS/Linux | Đổi tài khoản Antigravity hiện chỉ hỗ trợ Windows |

## Ghi chú

- Token của từng tài khoản nằm trong **Windows Credential Manager**; file `~/.agentdeck/antigravity-accounts.json` chỉ chứa email, cờ đang dùng và trạng thái — không chứa token.
- Đổi tài khoản không đăng xuất Google trong trình duyệt; nó chỉ đổi credential mà `agy` dùng.
- Tài khoản đang dùng luôn được lưu trước khi thêm tài khoản mới, nên chuyển lại được bất cứ lúc nào.
- Số liệu **Usage** của Antigravity chỉ có khi một tool ghi nó từ status line của `agy` (ví dụ AI Usage Meter); AgentDeck không đổi setting của CLI.
