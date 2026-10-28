# Store sync implementation

1. Add curated Cloudflare audit and opt-in external Chrome MCP entries to the existing cached catalog.
2. Reuse the skill library and scan/update pipeline; synchronize selected library skills into global provider folders or project folders through managed links, without overwriting user folders.
3. Reuse MCP discovery and add operations; fill missing global provider entries and merge project entries into Claude, Codex and Copilot configurations. Report conflicts and per-target failures.
4. Add a Store sync view with global/all-project/single-project targets, bulk and individual actions, update option and persistent results.
5. Verify Rust check/clippy, frontend check and whitespace. Confirm the browser connection prerequisite using upstream docs. Preserve all unrelated config.
