# Store: skill locations by provider

User requirement: skill install/sync/update must reach the AI provider's real skill folders, at global or project scope. Updating only an AgentDeck cache is insufficient. When adding providers, extend the mapping below and the runtime discovery together.

## Owning code and paths

`apps/desktop/src-tauri/src/store/library.rs` owns `SKILL_AGENTS`, `skill_folders` (global), `project_skill_folders` (per provider) and `project_folders` (deduplicated destinations for install/sync to all providers).

Paths below are AgentDeck's configured destinations, not a guarantee that every native CLI version supports every alias. Verify discovery against the installed runtime when adding/changing a provider.

| Provider | Global destination | Project destination |
| --- | --- | --- |
| Claude | `~/.claude/skills/<name>` | `<project>/.claude/skills/<name>` |
| Codex | `~/.codex/skills/<name>`; also reads `~/.agents/skills` | `<project>/.codex/skills/<name>` and `<project>/.agents/skills/<name>` |
| GitHub Copilot | `~/.copilot/skills/<name>` | `<project>/.github/skills/<name>` |
| OpenCode | `~/.config/opencode/skills/<name>`; legacy read alias `~/.config/opencode/skill` | `<project>/.opencode/skills/<name>` |
| Antigravity | `~/.gemini/config/skills/<name>` | `<project>/.agents/skills/<name>` |

OpenCode's native paths are documented at <https://opencode.ai/docs/skills>. Codex aliases and Antigravity paths above reflect the current repository reader; keep version-specific verification explicit rather than guessing a folder from the provider ID.

## Install, replace, remove

- The managed content lives at `~/.agentdeck/library/skills/<name>`. Provider destinations are directory junctions on Windows, symlinks elsewhere, pointing to that exact content. Both global and project installs must create their provider destinations; a cache entry alone is not an install.
- Update downloads and scans the new source, prepares a complete folder, then replaces the managed folder. Obsolete files are removed. Existing junctions resolve to the new content at the same path; do not replace the links with independent copies.
- Failure before replacement preserves the old skill. Replacement/metadata failures attempt rollback; retained backup paths are reported if rollback fails.
- Independently created real folders (`Own copy`) are not managed links. Do not silently overwrite, delete, or claim to update them. Explicit import/adoption is required before Store manages their content.
- Global assignment goes through `library::assign`. Project install, sync and removal use `project_folders()`; do not hardcode folder arrays in individual callers.
- Removing a managed skill unlinks only AgentDeck-owned provider destinations (including registered projects), then removes the managed content and source lock entry.
- Global/project links currently share one managed version. Replacing it updates every linked destination, even when the action was launched from one project. Per-project version isolation would require separate managed content and metadata.

## Checklist for every new provider

1. Verify native global/project locations and SDK discovery behavior for the version being integrated. Unsupported scope must be explicit; do not fall back silently to global.
2. Extend `SKILL_AGENTS`, `skill_folders`, `project_skill_folders` and this table.
3. Extend `slash_commands::list` and its allowed read roots. Configure SDK/runtime skill directories where native discovery is not automatic.
4. Extend provider registry and Store assignment UI, plus catalog compatibility filters. A picker entry alone is insufficient.
5. Verify install, replacement (including removed upstream files), disable/unassign, and removal for both scopes. Check native discovery and that independent folders survive.
6. Keep MCP configuration paths separate: skills directories are not MCP config locations. Use `mcp_manage.rs` and `mcp_providers.rs` for MCP scopes and enable/disable.
