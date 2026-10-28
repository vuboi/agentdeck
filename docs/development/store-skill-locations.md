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

## Editable sources and update badges

- A managed skill's pencil action edits its HTTPS Git repository and relative skill folder through `store_skill_source`. Keep the installed hash until an update succeeds; editing provenance must not claim that installed content changed. Resolve source folders inside the staged repository, including symlinks.
- An MCP's pencil action edits its command/arguments or endpoint through `mcp_source`. Preserve environment variables, HTTP headers, timeouts, provider-specific fields, scope and enabled state. Disabled definitions must remain in Credential Manager instead of becoming active during editing. Return only the connection fields needed by the editor.
- Store checks on opening and every 30 minutes while mounted. A manual refresh forces another check. Fetch each Git repository once per check, then compare its installed skill subfolders. Badge counts represent detected changes; checks never install automatically.
- Version detection for MCP currently supports stable explicit npm pins in `npx`, `pnpm dlx` and `bunx` arguments. Compare the configured pin with the npm registry's stable `latest` version. Floating tags such as `@latest`, unversioned/local commands, Python runners and HTTP servers have no inferred version badge. Do not show them as up to date without evidence.
- MCP package Update requires an explicit confirmation, rechecks the registry, changes only the package argument and preserves other configuration. New provider sessions use the new version. A provider-specific edit can partially succeed across files; report failures and refresh the current configuration.
