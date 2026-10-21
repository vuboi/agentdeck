# AgentDeck

Desktop app for driving several AI coding agents (Claude Code, Codex, Gemini) from one chat UI,
with an embedded preview, multi-agent workflows, quota tracking and an audit log.

- Core: Rust (`crates/`), daemon `agentd` + CLI `ag`
- Desktop: Tauri 2 + Svelte 5 (`apps/desktop`), UI built on `@shival99/z-ui-svelte`
- Plan and module order: [PLAN.md](PLAN.md). Conventions for coding agents: [AGENTS.md](AGENTS.md)

## Develop

```bash
pnpm install
pnpm dev        # tauri dev (needs the Rust toolchain; on Windows keep the repo path short)
pnpm check      # cargo check + clippy + svelte-check
pnpm test
pnpm gen:types  # regenerate packages/protocol-ts from the Rust protocol types
```

Status: M0 (scaffold) and M1 (protocol + event store) are done; the UI is a mock shell for now.
