---
name: repository-reuse
description: Use before creating helpers, utilities, services, wrappers, shared types, or refactors that may duplicate existing code.
---

# Repository Reuse

Use the least expensive repository search that answers the question:

1. Use `rg` for exact names, paths, config strings, translation keys, and dynamic references.
2. Use CodeGraph for symbols, callers, impact, and dependency flows.
3. Use Serena for symbol-level definitions, references, implementations, and TypeScript/Angular navigation.
4. Inspect one or two nearby implementations before creating an abstraction.
5. Reuse existing code when its behavior and contract match.
6. Keep code local when it is short, one-use, feature-specific, or still has an unstable contract.
7. Extract shared code only when it has meaningful reuse, a stable contract, or an independently changing responsibility.
8. Do not add generic wrappers that only rename native operations, such as `toArray`, `toObject`, `normalize`, or `mapData`.
9. Before adding an abstraction, record what was searched, why existing code cannot be reused, and why the new abstraction should change independently.
10. Intentional duplication is acceptable when it avoids premature coupling; state that decision briefly.
