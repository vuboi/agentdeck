# Store scoped skill and MCP sync

## Goal

Extend AgentDeck Store so users can install and synchronize skills and MCP servers at global or project scope, across the supported providers. Include Cloudflare's `security-audit-skill` in the initial skill catalog and a built-in Chrome DevTools MCP entry for controlling an external user browser. The browser MCP is opt-in.

## Current behavior

- Store keeps a managed skill library under `~/.agentdeck/library/skills` and links selected skills into provider user folders.
- Git-backed skills can be checked and updated one at a time after scanning the fetched content.
- MCP entries are discovered from provider user configs and project entries in Claude's config. Add/remove are provider CLI operations or Copilot config edits; there is no AgentDeck project-scoped canonical configuration.
- The Updates view currently updates skills only.

## Design

AgentDeck's Store remains the management surface and source of install metadata. Add explicit `global` and `project` scope to managed skills and MCP installations. Global skills continue to use the shared library and provider folders; project skills are installed under the project's standard `.agents/skills/<name>` directory so Codex, Claude, and Copilot can discover the same project skill. Preserve project scope in Store metadata so list, update, and removal target only that installation.

Represent managed MCP definitions with the existing transport fields and selected providers. Global definitions are synchronized to the provider's user-level configuration; project definitions are synchronized only to the selected project's provider-supported project configuration. Keep existing user entries intact and report per-provider failures. The UI exposes sync/update for an individual entry and a bulk action scoped to the selected project or global entries.

The bulk action refreshes Git-backed skills from their sources, re-runs the existing scan gate, and applies only safe/acknowledged updates. For MCP it reconciles the saved definitions into selected provider/project configs and reports package versions when they are pinned; it does not silently upgrade arbitrary runtime packages. Existing one-skill update remains available.

Add the Cloudflare Git repository as a built-in skill source/catalog item, pointing to the repository's verified skill subdirectory. Add Chrome DevTools MCP as a built-in Store item with its documented package command, use the user's external Chrome only when explicitly enabled, and route install through the existing staging/review/acknowledgement flow. No MCP server starts merely because it is listed in the catalog.

## Boundaries and safety

- Keep all changes in the existing Store, skill library/update, MCP manager, and their UI/state files; do not add another Store or provider registry.
- Preserve current provider-owned config entries, fail closed on malformed config, and return per-provider results rather than claiming a full sync when any target fails.
- Never return secret values to the UI. Project files receive MCP configuration only when the user selects project scope.
- MCP package commands remain visible and require the existing install acknowledgement. Built-in browser MCP remains disabled until explicitly installed/enabled.

## Verification

- Rust tests cover scope routing, merge/preserve behavior, and bulk update results without network access.
- Frontend `pnpm --filter desktop check` validates scoped controls and status reporting.
- Run the project's applicable desktop Rust check/clippy and `git diff --check`.
- Verify catalog metadata and documented Chrome DevTools MCP launch configuration against the upstream sources before embedding them.

## Open implementation detail

Project-local discovery uses `.agents/skills` for skills. MCP project config targets must follow each provider's supported format; unsupported project-level targets are surfaced as unsupported and are not silently written to a global file.
