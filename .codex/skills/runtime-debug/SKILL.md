---
name: runtime-debug
description: Diagnose runtime errors from an existing VS Code terminal using live terminal, browser, and network evidence without restarting the user's process.
---

# Runtime debugging from an existing terminal

Use when the user reports that the app is broken while `pnpm run` is already running in a VS Code terminal.

1. Read the current terminal output, browser console, and network evidence before asking the user to paste logs.
2. Inspect the relevant source, stack trace, browser console, and network evidence when available.
3. Fix the root cause in source or configuration.
4. Do not kill, stop, restart, or take ownership of the user's running terminal process.
5. Do not launch a replacement `pnpm run` command unless the user explicitly asks.
6. Tell the user to let the existing dev server hot-reload, or ask them to reload the browser when appropriate.
7. If evidence is unavailable, ask for a fresh reproduction or the smallest relevant log excerpt.
