<div align="center">

<img src="apps/desktop/src-tauri/icons/128x128@2x.png" alt="AgentDeck" width="96" height="96" />

# AgentDeck

**One desktop chat for all your AI coding agents.**
Drive Claude Code, Codex and Antigravity side by side, with a live preview browser, multi-agent flows, usage limits and an audit log.

[![Latest release](https://img.shields.io/github/v/release/vuboi/agentdeck?style=for-the-badge&logo=github&label=release&color=8b5cf6)](https://github.com/vuboi/agentdeck/releases/latest)
[![Downloads](https://img.shields.io/github/downloads/vuboi/agentdeck/total?style=for-the-badge&logo=docusign&logoColor=white&color=22c55e)](https://github.com/vuboi/agentdeck/releases)
[![Build](https://img.shields.io/github/actions/workflow/status/vuboi/agentdeck/release.yml?style=for-the-badge&logo=githubactions&logoColor=white&label=build)](https://github.com/vuboi/agentdeck/actions/workflows/release.yml)
[![Stars](https://img.shields.io/github/stars/vuboi/agentdeck?style=for-the-badge&logo=github&color=eab308)](https://github.com/vuboi/agentdeck/stargazers)
[![License](https://img.shields.io/github/license/vuboi/agentdeck?style=for-the-badge&color=64748b)](Cargo.toml)

![Windows](https://img.shields.io/badge/Windows-0078D4?style=flat-square&logo=windows&logoColor=white)
![macOS](https://img.shields.io/badge/macOS-000000?style=flat-square&logo=apple&logoColor=white)
![Linux](https://img.shields.io/badge/Linux-FCC624?style=flat-square&logo=linux&logoColor=black)
![Tauri](https://img.shields.io/badge/Tauri_2-24C8DB?style=flat-square&logo=tauri&logoColor=white)
![Svelte](https://img.shields.io/badge/Svelte_5-FF3E00?style=flat-square&logo=svelte&logoColor=white)
![Rust](https://img.shields.io/badge/Rust-000000?style=flat-square&logo=rust&logoColor=white)

[**Download**](https://github.com/vuboi/agentdeck/releases/latest) · [Install](#install) · [Features](#features) · [Develop](#develop)

<br />

<img src="docs/images/chat.png" alt="AgentDeck chat view" width="900" />

</div>

---

## Features

- **Many agents, one chat.** Claude Code, Codex and Antigravity in the same window. Switch agent in the middle of a chat: each reply keeps the agent that wrote it, and the new agent gets the turns it missed.
- **Your history, imported.** Existing Claude Code and Codex chats show up by project, ready to continue.
- **Live steps.** Commands fold into one counting line ("Ran 5 commands"), thinking sits inside it, file edits show coloured diffs, and the agent's plan appears as a checklist card.
- **Run in terminal.** Shell code blocks in the chat have a **Run** button that runs the command in the built-in terminal.
- **Embedded preview browser.** The agent can see and operate your page (click, type, screenshot, console, network), and you can mark up the live page. Developer tools (Network, Application, Console) are docked at the bottom.
- **Multi-agent flows.** Build flows on a canvas: one agent writes, another reviews, the owner fixes what blocks. Flow templates and per-flow history included.
- **Usage limits.** Claude and Codex limit status, token and cost counters, and a warning when you are near a limit.
- **MCP in one place.** Manage MCP servers for every agent from one list.
- **Audit log.** Every tool call and error is kept on disk, grouped by day, with the screenshots tools returned.
- **Built-in updates.** The app tells you when a new version is out and installs it for you.

## Install

Pick your system on the [**latest release**](https://github.com/vuboi/agentdeck/releases/latest) page.

| System | Download | Notes |
| --- | --- | --- |
| **Windows** 10/11 | `AgentDeck_x64-setup.exe` (or `.msi`) | Run the installer. If SmartScreen warns, choose *More info → Run anyway*. |
| **macOS** Apple silicon | `AgentDeck_aarch64.dmg` | Open the `.dmg`, drag AgentDeck to *Applications*. |
| **macOS** Intel | `AgentDeck_x64.dmg` | Same as above. |
| **Linux** | `AgentDeck_amd64.AppImage`, `.deb` or `.rpm` | See below. |

<details>
<summary><b>macOS: "AgentDeck is damaged" or "cannot be opened"</b></summary>

The app is not notarized yet. Right-click AgentDeck in *Applications* and choose **Open** once, or run:

```bash
xattr -dr com.apple.quarantine /Applications/AgentDeck.app
```

</details>

<details>
<summary><b>Linux</b></summary>

```bash
# AppImage
chmod +x AgentDeck_*.AppImage && ./AgentDeck_*.AppImage

# Debian / Ubuntu
sudo apt install ./AgentDeck_*.deb

# Fedora / RHEL
sudo dnf install ./AgentDeck-*.rpm
```

</details>

**Before you start:** install the agent CLIs you want to use (for example [Claude Code](https://docs.claude.com/en/docs/claude-code) and [Codex](https://github.com/openai/codex)) and sign in to them once. AgentDeck uses the CLIs you already have.

### Updates

AgentDeck checks for a new version when it starts and every few hours. A dot appears on the download button next to **Settings**, in the bottom-left corner. Click it, read what changed, then **Download and restart**. Updates are signed; a build with a bad signature is refused.

## Develop

Needs Node 24, pnpm 10 and the Rust toolchain.

```bash
pnpm install
pnpm dev        # tauri dev (on Windows keep the repo path short)
pnpm check      # cargo check + clippy + svelte-check
pnpm test
pnpm gen:types  # regenerate packages/protocol-ts from the Rust protocol types
```

- Core: Rust (`crates/`), desktop: Tauri 2 + Svelte 5 (`apps/desktop`), UI on `@shival99/z-ui-svelte`.
- Plan and module order: [PLAN.md](PLAN.md). Conventions for coding agents: [AGENTS.md](AGENTS.md).

## Release

1. Write the notes in `docs/release-notes/vX.Y.Z.md` (if the file is missing, the notes are built from the commit titles since the last tag).
2. Tag and push:

   ```bash
   git tag vX.Y.Z && git push origin vX.Y.Z
   ```

3. The [release workflow](.github/workflows/release.yml) builds Windows, macOS (Apple silicon and Intel) and Linux, signs the updates and publishes the release. Installed apps pick it up on their next check.

One-time setup: add the repository secret `TAURI_SIGNING_PRIVATE_KEY` (the updater signing key).

## License

[MIT](Cargo.toml)
