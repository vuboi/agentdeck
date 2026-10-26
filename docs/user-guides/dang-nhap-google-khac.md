# Đăng nhập thêm tài khoản Google (Antigravity)

AgentDeck giữ nhiều tài khoản Google cho Antigravity (`agy`) và đổi qua lại mà không phải đăng xuất CLI. Mỗi lần thêm tài khoản, AgentDeck lưu token của tài khoản đang dùng trước, rồi mở phiên Google mới.

> Chỉ chạy trên **Windows** (token lưu trong Windows Credential Manager) và cần đã cài `agy`.

## Đường nhanh

```
Ctrl+,  →  Connectors  →  thẻ Antigravity  →  Sign in another account
```

## Vị trí các nút

**Settings › Connectors** (khi đã có ít nhất một tài khoản):

```
┌────────────────────────────────────────────────────────────────────────┐
│ Settings                                                               │
│ Connectors     Antigravity                                       [ X ] │
│ Providers      Google's agent CLI (agy) …                              │
│ Limit alerts   ──────────────────────────────────────────────────────  │
│ Sync           ANTIGRAVITY ACCOUNTS        [ Sign in another account ] │
│ Computer use   ban@gmail.com                                    IN USE │
│                viec@gmail.com                                      [⇄] │
└────────────────────────────────────────────────────────────────────────┘
```

`[⇄]` là nút **Use this account** ở cuối hàng. `[X]` đóng Settings.

Khi **chưa đăng nhập tài khoản nào**, nút **Sign in** nằm ở góc phải trên của thẻ Antigravity; nút **Sign in another account** chưa hiện.

**Thanh dưới ô nhập tin nhắn** (chat đang dùng agent Antigravity):

```
[ project ▾ ]  [ branch ▾ ]           [ ⇄ ban@gmail.com ▾ ]
                                      ↑ chip tài khoản, cuối thanh bên phải
```

| Nút | Vị trí | Dùng khi |
| --- | --- | --- |
| **Settings (Ctrl+,)** | góc dưới trái cửa sổ, cạnh nút cập nhật | mở Settings |
| **Connectors** | mục đầu tiên, cột trái Settings | xem CLI và tài khoản |
| **Sign in** | góc phải trên thẻ Antigravity | chưa có tài khoản nào |
| **Sign in another account** | cuối hàng “Antigravity accounts”, bên phải | đã có 1 tài khoản, thêm tài khoản Google khác |
| **+** | góc phải tiêu đề popover tài khoản (dưới ô chat) | thêm nhanh tài khoản |
| **chip email ⇄** | cuối thanh dưới ô nhập tin nhắn | đổi tài khoản nhanh |
| **⇄ Use this account** | cuối hàng tài khoản trong Settings | đổi tài khoản trong Settings |
| **Open sign-in** → **Continue and switch** | hộp thoại “Sign in to Antigravity” | token cũ hết hạn |
| **Switch account** | hộp thoại “Switch account?” | xác nhận đổi tài khoản |

## Các bước thêm tài khoản Google khác

1. Mở **Settings** (`Ctrl+,`) → **Connectors**. Thẻ **Antigravity** phải đang **Signed in**.
2. Bấm **Sign in another account** (cuối hàng “Antigravity accounts”).
3. AgentDeck lưu token tài khoản hiện tại, xoá phiên cũ, rồi mở **cửa sổ console Antigravity CLI** và **trang đăng nhập Google** trong trình duyệt.
4. Trên trang Google:
   - chọn tài khoản muốn thêm nếu email đã có trong danh sách;
   - nếu chưa, bấm **Use another account** (Dùng tài khoản khác) ở cuối danh sách → nhập email → **Next** → mật khẩu → **Next**;
   - đồng ý quyền nếu Google hỏi.
5. Xong, quay lại AgentDeck. Tài khoản được lưu tự động và xuất hiện trong “Antigravity accounts”; tài khoản vừa đăng nhập mang nhãn **IN USE**. Đóng cửa sổ console là được.
6. Nếu danh sách chưa đổi, bấm **Refresh** ở góc phải trên Settings, hoặc đóng rồi mở lại Settings.

## Đổi giữa các tài khoản

- **Nhanh:** bấm **chip email** ở cuối thanh dưới ô nhập tin nhắn → chọn tài khoản → hộp thoại **Switch account?** → **Switch account**.
- **Trong Settings:** hàng tài khoản → nút **⇄ Use this account** ở cuối hàng.

Nhãn **IN USE** chuyển sang tài khoản mới. Lần chạy `agy` tiếp theo dùng tài khoản đó.

## Khi trục trặc

| Hiện tượng | Cách xử lý |
| --- | --- |
| Tài khoản có nhãn **Needs sign-in** | Chip tài khoản dưới ô chat → dòng đó (nhãn **Sign in**) → **Open sign-in** → đăng nhập đúng email đó → **Continue and switch** |
| Đổi tài khoản báo “the Antigravity sign-in has expired” | Token đã hết hạn: đăng nhập lại tài khoản đó rồi chuyển lại |
| “Sign-in completed, but the account was not saved” | Đóng rồi mở lại hộp thoại để thử lại; hoặc bấm **Retry save** ở hàng “Antigravity accounts” |
| Không thấy **Sign in another account** | Chưa đăng nhập tài khoản nào (dùng **Sign in** góc phải thẻ), hoặc `agy` chưa cài (bấm **Install**) |
| Không thấy chip tài khoản dưới ô chat | Chat chưa chọn agent Antigravity, hoặc `agy` chưa cài |
| Báo lỗi Credential Manager trên macOS/Linux | Đổi tài khoản Antigravity hiện chỉ hỗ trợ Windows |
| Đăng nhập quá 5 phút không xong | AgentDeck dừng chờ; chạy lại **Sign in another account** |

## Ghi chú

- AgentDeck lưu token của từng tài khoản trong **Windows Credential Manager**; danh sách email/trạng thái nằm ở `~/.agentdeck/antigravity-accounts.json` (không chứa token).
- Đổi tài khoản không đăng xuất Google trong trình duyệt; nó chỉ đổi credential mà `agy` dùng.
- Tài khoản đang dùng vẫn được lưu trước khi thêm tài khoản mới, nên có thể chuyển lại bất cứ lúc nào.
