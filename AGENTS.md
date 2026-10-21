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
