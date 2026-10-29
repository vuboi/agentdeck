# Platform support: what differs by operating system

AgentDeck runs on Windows, macOS and Linux. This page lists every place in the Rust code that behaves differently by
system (found with `cfg(windows)`, `cfg(unix)`, `cfg(target_os = …)` and `cfg!(…)`: about 160 places in 25 files), and its
state on each. Update it when you add or change one. It was written from a Windows machine: the macOS and Linux columns
say what the code does, not that it was run there.

Legend: **ok** written and (where noted) tested; **win only** no equivalent yet; **untested** code exists but was never
run on that system.

## Rules for new code

1. Start a long-lived process (agent CLI, MCP server, shell, server) → call `process_guard::adopt_pid(pid)` right after it
   starts, so it ends with the app even when the app is killed.
2. End a process and what it started → `process_guard::kill_tree(pid)`, never `child.kill()` alone.
3. Start a unix process → `process_group(0)` so the group can be ended; on Windows add `CREATE_NO_WINDOW` (`0x0800_0000`).
4. Paths: use `path_links.rs` (`open_launch`, `reveal_launch`) and `src/lib/files/path-links.ts`; do not cut paths by hand.
5. Home folder: `USERPROFILE` on Windows, `HOME` elsewhere (see `path_links.rs` `home()`).
6. A branch that exists for one system only gets a line in the table below, with what the others do.

## Process lifetime

| Place | Windows | macOS | Linux |
| --- | --- | --- | --- |
| Agent CLI turns (`crates/adapters/.../runner.rs`, `warm.rs`) | `cmd /C` around `.cmd` shims, no console window, job object | own process group, `kill -KILL -pgid` | same as macOS |
| Ends with the app when the app is killed (`process_guard.rs`) | job object `KILL_ON_JOB_CLOSE` (ok) | `sh` watchdog, 2 s poll (untested) | `sh` watchdog, 2 s poll (untested) |
| Ends with the app on a normal exit (`lib.rs` `RunEvent::Exit`) | warm pool, servers, shells | same | same |
| Dev servers (`servers.rs`) | job object, `cmd /C` | process group, `sh -c` | same |
| Terminals (`pty.rs`) | ConPTY, job, `taskkill /T /F` | pty, group kill | pty, group kill |
| MCP servers (`mcp_client.rs`), custom chat (`custom_chat.rs`) | job | watchdog | watchdog |
| Login and install tasks (`connectors.rs`) | `CancellableChild`, `cmd` wrappers | plain child | plain child |

Known gaps: the watchdog polls every 2 seconds, so a process can outlive a killed app by up to 2 s; Linux could use
`PR_SET_PDEATHSIG` for the direct child as well. A process started with no guard (login tasks, `devtools.rs`) is not tied
to the app.

## Files, folders and the system

| Feature | Windows | macOS | Linux |
| --- | --- | --- | --- |
| Open a file, reveal in the file manager (`path_links.rs`) | `cmd /C start`, `explorer /select,"…"` (tested builder) | `open`, `open -R` (tested builder) | `xdg-open`, parent folder (tested builder) |
| Open with an application (`lib.rs`) | per-app search, `which_app` | `open -a` | per-app, `xdg-open` |
| Reveal a folder (`lib.rs` `reveal_path`) | `explorer` | `open` | `xdg-open` |
| Existence check (`path_links.rs`) | as written | tries NFC and NFD spellings | as written, case-sensitive |
| Keep awake (`awake.rs`) | power request API | `caffeinate` | see file |
| Notifications (`notify.rs`) | toast with the AgentDeck logo | plugin default | plugin default |
| Null device in git calls (`changes.rs`, `store/stage.rs`) | `NUL` | `/dev/null` | `/dev/null` |

## Features that are Windows-only today

| Feature | Where | What the others do |
| --- | --- | --- |
| Browser extension manager | `browser.rs` `EXTENSIONS_ENABLED`, `browser_extensions.rs` | hidden (no WebView2 extension API) |
| WebView2 Preview internals (cookies, profile, devtools) | `browser.rs`, `cdp.rs` | separate non-Windows branches (untested) |
| Computer use (screen, keys, mouse) | `computer.rs` | non-Windows branch using `enigo` (untested) |
| Image generation helpers | `image_gen.rs` (6 places) | console-window flags are Windows only; the rest is shared |

## Tools that need a different binary or flag

| Place | Windows | macOS / Linux |
| --- | --- | --- |
| Piper voice (`tts.rs`) | `piper_windows_amd64.zip`, `piper.exe` | `piper_macos_*.tar.gz` / `piper_linux_*.tar.gz`, `piper` (tested names, never run) |
| Default shell of a terminal (`pty.rs`) | PowerShell / `cmd` | `$SHELL` |
| Dev tool runner (`devtools.rs`) | `cmd /C` | direct program |
| CLI detection (`detect.rs`), binary suffixes (`connectors.rs`, `lib.rs`) | `.exe`, `.cmd` | no suffix |
| PATH of a CLI started from the desktop (`resolve.rs`) | inherited | login-shell PATH added (a desktop app starts without it) |
| OpenCode, Antigravity adapters | Windows flags | none |

## Checking a change

```bash
cargo clippy --workspace -- -D warnings
cargo test -p agentdeck-desktop --lib process_guard path_links tts
```

The unix-only test `process_guard::tests::a_process_ends_when_its_app_is_gone` runs on macOS and Linux only. Windows CI
cannot run it; run it once on each system after changing `process_guard.rs`.
