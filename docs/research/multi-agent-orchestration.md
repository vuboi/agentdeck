# Multi-agent orchestration: what others do, and what AgentDeck should change

Research date: 2026-10-24. Scope: coordinating several agent CLIs (Claude Code, Codex, Antigravity, local models) in AgentDeck flows. Baseline is the current design in `apps/desktop/src/lib/sessions.svelte.ts` (`runWorkflow`, `handBack`): each step gets its job, the task, BOARD.md and only the `## Handoff` sections of the steps that feed it. Steps keep `agents/<name>.md` notes (`[F]`, `[Q @x]`, `[A @x]`). The main agent is called once at the end with all handoffs plus the `[F]` lines. "Unverified" marks claims I could not check against a primary source.

## 1. Inside one vendor: Claude subagents, agent teams, orchestrator-worker

- **Anthropic's research system** ([engineering post](https://www.anthropic.com/engineering/multi-agent-research-system)) uses orchestrator-worker: an Opus 4 lead with Sonnet 4 subagents scored 90.2% better than single-agent Opus 4 on Anthropic's internal research eval. The cost was high. Agents use about 4x the tokens of chat, and multi-agent systems about 15x. Token usage alone explained about 80% of the performance variance on BrowseComp; tokens, tool calls and model choice together explained 95%. Lessons that apply to AgentDeck:
  - A delegation must state the **objective, output format, tools/sources and task boundaries**. Vague tasks led to duplicated work.
  - Subagents write results to external storage and pass **"lightweight references"** back. This avoids the "game of telephone" through the lead.
  - **Scale effort to the task**: 1 agent with 3-10 tool calls for a simple fact, 2-4 subagents for comparisons, more than 10 only for complex research.
  - The post calls **coding a poorer fit** than research: fewer parts can run in parallel, and agents depend more on shared context.
- **Claude Code subagents** ([docs](https://code.claude.com/docs/en/sub-agents)): each runs in its own context and **returns only its final report**. Its frontmatter sets `model` (`haiku`/`sonnet`/`opus`/`inherit`) and the allowed `tools`. Built-in Explore is read-only and has quick/medium/very-thorough levels. Subagents nest up to 3 levels and can be resumed with `SendMessage`. Anthropic's [context-engineering post](https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents) says a subagent may spend tens of thousands of tokens but returns "often 1,000-2,000 tokens". It also recommends structured notes (a NOTES.md or to-do file) and keeping file paths as references, loaded just in time.
- **Agent teams** ([docs](https://code.claude.com/docs/en/agent-teams), experimental): a lead, teammates, a **shared task list with dependencies** (claims use file locking) and a **per-agent JSON mailbox**. Teammates can be made to plan first and send a "plan approval request". `TaskCompleted` and `TeammateIdle` hooks act as quality gates (exit code 2 sends feedback). Guidance: 3-5 teammates, 5-6 tasks each, and each teammate owns its own files. The [costs page](https://code.claude.com/docs/en/costs) says teams use "approximately 7x more tokens than standard sessions when teammates run in plan mode" and recommends Sonnet for teammates.
- **Cognition** reversed course in part. ["Don't Build Multi-Agents"](https://cognition.ai/blog/dont-build-multi-agents) (June 2025) argued that summaries lose upstream decisions. The April 2026 follow-up, ["Multi-Agents: What's Actually Working"](https://cognition.com/blog/multi-agents-working), finds that **multi-agent works when only one agent writes**. A **clean-context reviewer that sees only the diff** found about 2 bugs per Devin PR, about 58% of them severe. The "smart friend" pattern lets a cheaper model consult a stronger one, but it failed when the primary was much weaker.

## 2. Across vendors: projects and how alive they are (GitHub, checked 2026-10-24)

| Project | What it does | Activity |
|---|---|---|
| [ruvnet/ruflo](https://github.com/ruvnet/ruflo) (ex claude-flow) | Swarm harness over Claude Code/Codex: shared memory, federation | ~79k stars, pushed today. Its benchmark claims (84.8% SWE-bench, 75% savings) are **unverified** |
| [smtg-ai/claude-squad](https://github.com/smtg-ai/claude-squad) | TUI that runs Claude Code, Codex, OpenCode and Amp in tmux + git worktrees | 8.7k stars, last push 2026-08 |
| [kbwo/ccmanager](https://github.com/kbwo/ccmanager) | Session manager for Claude/Gemini/Codex/Cursor/Copilot/Cline/OpenCode/**Kimi CLI** | pushed 2026-10-18 |
| [BeehiveInnovations/pal-mcp-server](https://github.com/BeehiveInnovations/pal-mcp-server) (ex zen-mcp) | MCP server; its `clink` tool runs another CLI as an isolated subagent with a role (planner, codereviewer). Example: Claude spawns Codex, which spawns Gemini | 11.8k stars, **last push 2025-12** (slowing) |
| [openai/codex](https://github.com/openai/codex) `codex mcp-server` | Codex as an MCP server: `codex()` starts a thread, `codex-reply()` continues it ([guide](https://developers.openai.com/codex/guides/agents-sdk)) | very active |
| [OpenHands](https://github.com/OpenHands/OpenHands) | Agent platform with delegation | very active |
| [crewAI](https://github.com/crewAIInc/crewAI) | Roles, tasks, crews; per-agent `max_iter` | very active |
| [LangGraph](https://github.com/langchain-ai/langgraph) | Graph and supervisor patterns. `langgraph-supervisor-py` is **archived** (2026-07); build supervisors in core LangGraph | active |
| [microsoft/autogen](https://github.com/microsoft/autogen) | README says it is moving to archive; use [Agent Framework](https://github.com/microsoft/agent-framework) (A2A + MCP) instead | maintenance only |
| [MetaGPT](https://github.com/FoundationAgents/MetaGPT) | SOP roles that pass structured documents (PRD, design) | last push 2026-01 (slowing) |
| [A2A](https://github.com/a2aproject/A2A) | Agent Card, Task, Message, Part, **Artifact**. Now under the Linux Foundation. A v1.0 in 2026 is reported but **unverified** | active |
| [AGENTS.md](https://github.com/agentsmd/agents.md) | One instruction file read by Codex, Cursor, Jules, Amp and others | active |
| [stravu/crystal](https://github.com/stravu/crystal) | Parallel Codex/Claude in worktrees (now "Nimbalyst") | last push 2026-02 |

Most of these tools are either **session multiplexers** (squad, ccmanager, crystal: parallel worktrees, a human compares the results) or **agent-as-tool bridges** (pal `clink`, `codex mcp-server`: one CLI calls another and gets back only the final answer). None I found standardizes the handoff content between vendors. Each one uses prose in prompts. A2A's Artifact is the closest thing to a standard envelope.

## 3. Compact inter-agent communication

Evidence:
- **References, not pasted content.** Anthropic (artifacts plus lightweight references), A2A Artifacts, MetaGPT documents and Claude teams' task files all point the same way. AgentDeck's run folder already does this.
- **Structure helps when it carries context, not when it constrains thinking.** [TalkHier](https://arxiv.org/abs/2502.11098) structures each message as message + intermediate output + background. Its ablations show that removing the structure hurts (MMLU 88.38% vs AgentVerse 83.66%). [Let Me Speak Freely?](https://arxiv.org/abs/2408.02442) found that strict JSON/XML formats **degrade reasoning**, while they can help classification-like extraction. So: reason in prose, and put only the final handoff in a small fixed schema.
- **Terse is cheap and roughly as good.** [Chain of Draft](https://arxiv.org/abs/2502.18600) matched chain-of-thought accuracy using as little as 7.6% of the tokens, by writing about 5-word drafts per step. [AgentPrune](https://arxiv.org/abs/2410.02506) (ICLR 2025) cut 28-73% of tokens by pruning redundant messages between agents. [Optima](https://arxiv.org/abs/2410.08115) trained agents toward efficient messages and reports up to 2.8x performance with under 10% of the tokens. That needs fine-tuning, so it does not fit an app driving stock CLIs.
- **A private "agent language".** Optima's reward explicitly keeps messages *readable*. I found no evidence that an untrained, invented DSL helps prompted frontier models. The risks are misreading between vendors and outputs no human can audit (**unverified as measured; my inference**). Telegraphic English with a few fixed tags (like the existing `[F]`/`[Q]`/`[A]`) is the safe middle ground.
- **Where multi-agent systems fail.** [MAST](https://arxiv.org/abs/2503.13657) (1600+ traces, 7 frameworks): specification issues 41.8%, inter-agent misalignment 36.9%, verification 21.3%. The top modes: step repetition 17.1%, reasoning-action mismatch 14.0%, **failure to ask for clarification 11.7%**, disobeying the task spec 11.0%, unaware of termination conditions 9.8%. Adding a high-level verification step gave +15.6% on ChatDev. These are the failures the handoff schema, the question protocol and the loop rules below target.

## 4. Clarifying questions: the native shapes

**Claude Code `AskUserQuestion`** ([SDK docs](https://code.claude.com/docs/en/agent-sdk/user-input), [tools reference](https://code.claude.com/docs/en/tools-reference)):
- `questions[]`, 1-4 per call. Each has `question`, `header` (≤12 characters), `options[]` (2-4 items, each `{label, description}`, optional `preview`) and `multiSelect`.
- Answers are returned as `answers: { [questionText]: label | label[] }`, plus an optional free-form `response`.
- The UI adds an "Other" row for free text. The tool is not available inside subagents. In the SDK it arrives through `canUseTool`.

**Codex `request_user_input`** (primary source: [`request_user_input_spec.rs`](https://github.com/openai/codex/blob/main/codex-rs/core/src/tools/handlers/request_user_input_spec.rs), [protocol](https://github.com/openai/codex/blob/main/codex-rs/protocol/src/request_user_input.rs)):
- `questions[]`, "prefer 1, do not exceed 3". Each has `id` (snake_case), `header` (≤12 characters), `question` (one sentence) and `options[]`.
- Options are 2-3 mutually exclusive `{label (1-5 words), description (one sentence on the tradeoff)}`. The recommended option goes first and is labelled "(Recommended)".
- The model must not add "Other": the client sets `isOther=true` and adds free text itself. There is also `isSecret`.
- Answers come back as `{answers: {id: {answers: string[]}}}`.
- The tool is gated to **Plan mode** unless the `default_mode_request_user_input` feature flag is set ([forum](https://community.openai.com/t/feature-request-allow-request-user-input-in-default-mode/1395392); flag status **unverified** in official docs).

Antigravity and local models: I found no documented structured-question tool (**unverified**). This is why a text protocol is needed.

**Proposed protocol (any CLI, plain text).** This is the common subset of both tools; the field names follow Codex's, which are the stricter of the two.

````
```agentdeck-ask
{"questions":[
  {"id":"storage","header":"Storage","question":"Where should run notes live?",
   "multi":false,
   "options":[
     {"label":"Run folder (Recommended)","description":"Keeps BOARD.md beside notes; easy to inspect."},
     {"label":"App database","description":"Hidden from the repo; needs a viewer."}]}
]}
```
````

Rules for the agent (one short paragraph in the prompt):
- At most 3 questions, 2-4 options each, `header` ≤12 characters.
- Put the recommended option first.
- Never write an "Other" option; the app always adds a free-text field.
- Emit the block **only before planning or editing**, as the last thing in the reply, then stop.

The app parses the **last** `agentdeck-ask` fence. If the JSON is invalid, it shows the raw text and a reply box. It answers with a normal user turn:

````
```agentdeck-answers
{"storage":["Run folder (Recommended)"],"_note":"free text if any"}
```
````

When the CLI has a native tool (Claude `AskUserQuestion` in stream-json, Codex `request_user_input` events), render the same wizard from the native event and answer natively. The fence is the fallback for Antigravity, local models and Claude/Codex in modes where the tool is off.

## 5. Plan/implement loops and caps

- **Plan approval as a message**: Claude teams send a plan-approval request from teammate to lead, and the lead approves or "sends it back for revision" ([docs](https://code.claude.com/docs/en/agent-teams)). This is the bounce-back channel.
- **Evaluator-optimizer**: generator, then a critic, then a fix. [Self-Refine](https://arxiv.org/abs/2303.17651) shows most of the gain comes in the first 2-4 rounds, while cost grows linearly. A 2026 study ([ICLR](https://www.iclr.cc/virtual/2026/10014641), not read in full) reports that self-refine on code correctness can *regress* through hallucinated fixes. So caps should be small and the critic should be external (tests, or a fresh reviewer).
- **Reviewer with a clean context, single writer** (Cognition): the reviewer sees the diff and the request, not the coder's history. A filter drops findings that are out of scope or contradict user instructions, to prevent loops.
- **Caps in frameworks**: CrewAI `max_iter` (docs say 25 in one place and 20 in another) and Claude teams' `TaskCompleted` exit-2 gate. OpenAI Agents SDK `max_turns` (10) and LangGraph `recursion_limit` (25) are from memory and **unverified** here.
- **Architect/editor split**: [Aider](https://aider.chat/2024/09/26/architect.html) reached 85% on its editing benchmark with o1-preview as architect and DeepSeek/o1-mini as editor, against 75.2% for o1-preview alone. A strong planner plus a cheaper, format-reliable implementer is a measured win.

## 6. Token economics

- **Multi-agent costs more in total.** Anthropic measured 15x chat for multi-agent and 4x for a single agent, and Claude teams are about 7x. It pays off when the work is parallel and valuable, or when isolating context **keeps the expensive model's context small**. A subagent that burns 50k tokens on Haiku/Sonnet and returns 1-2k saves the Opus/strong context from carrying 50k on every later turn.
- **Caching dominates real bills.** Claude Code re-sends the whole history every request, at cache-read rates (AgentDeck's `usage.ts` prices cache reads at 0.05-0.1x input). Each new step **starts cold**: its system prompt, CLAUDE.md and tool definitions are cache writes (1.25x), and the 5-minute TTL expires between slow stages ([costs](https://code.claude.com/docs/en/costs)). Many short steps pay this cold start many times. The `/usage` breakdown in Claude Code shows "91% of input tokens from cache" in the main session as an example. Splitting work lowers that rate.
- **Model per role, from the sources.** Use a strong model to plan and own the result (Anthropic lead=Opus, Aider architect). Use Sonnet-class for implementation and teammates. Use Haiku/cheap or local for search, summarizing and listing files (`model: haiku` for simple subagents). For review, Cognition used a frontier model with a clean context. Review is cheap in tokens because the input is only the diff, so do not downgrade it too far.
- **No source gives a general break-even point.** The only measured rule is Anthropic's "token usage explains about 80% of variance": spending more tokens buys quality. AgentDeck has to measure its own flows (below).

## Recommendations for AgentDeck (prioritized)

1. **Handoff as a fixed, small schema; reasoning stays prose.** Keep `## Handoff`, but make the lines machine-parseable so the app stops cutting at 1600 characters:
   ```
   ## Handoff
   status: done | blocked | needs-plan-change
   result: <1-3 sentences>
   changed: path/a.ts, path/b.rs
   facts:
   - <fact> (path:line)
   open:
   - <issue>
   verify: <command run> -> pass|fail
   ```
   Parse `status` and `changed` in the app. Send the downstream step only the handoff plus **file paths** (not contents). Add `verify:` so the reviewer and owner know what was actually run (this targets MAST's verification failures).
2. **Brief = objective, output format, boundaries, done-criteria.** Anthropic's four elements. Add "Out of scope:" and "Done when:" lines to each step's brief. Drop the full BOARD.md content from the brief and give its path plus the goal and plan lines; the current code inlines the whole board for every step.
3. **Question protocol** (section 4): add the `agentdeck-ask` fence rule to Plan-type steps and to the main chat. Render native `AskUserQuestion`/`request_user_input` events with the same wizard. Ask only *before* the first stage; mid-run questions pause the run and show in the step card.
4. **Bounce-back with caps.** `status: needs-plan-change` from an implementer re-runs the planner once with the implementer's handoff. A reviewer returning `status: blocked` with findings runs a **fixer** (the same implementer session resumed, which keeps its cache warm) and then a **fresh** reviewer. Caps: plan bounce ≤1, review→fix ≤2 rounds per run. After that, hand back to the main agent and the user with the open findings. Filter reviewer findings against the user's request before looping (Cognition).
5. **Single writer per file set.** Steps in the same stage must declare the files they own in BOARD.md. Parallel stages should be read-only (research, review) unless their file sets do not overlap. This matches Cognition's and the teams docs' file-ownership rule.
6. **Model-per-role defaults in presets.** Plan and owner: strongest model. Implement: Sonnet-class or Codex. Search/summarize: Haiku or local. Review: a strong model from a *different vendor* than the implementer, fed only the diff and the request. Show the estimated cost per role in the preset editor.
7. **Measure split vs single.**
   - Per step, store input, cache-read, cache-write and output tokens separately (the data already exists in `Message.tokens`), cost, wall time, and a **cache hit rate** = cacheRead / (input + cacheRead + cacheWrite).
   - Per run, add **orchestration overhead** = tokens of the handback turn + brief tokens of all steps (estimate at about 4 characters per token), and **outcome**: checks passed (the `verify:` line), the reviewer's verdict, and whether the user accepted or re-asked within 10 minutes.
   - Add an **A/B mode**: re-run the same task as a single main-agent chat (on a worktree) and compare cost per accepted outcome. Without an outcome signal, token totals mislead, because more tokens usually buy quality.
   - Flag steps whose cache-write share is above about 40%: these are cold-start-dominated and candidates for merging.
8. **Keep notes telegraphic, no invented DSL.** Keep `[F]/[Q @x]/[A @x]`, add `[D]` for decisions, and cap each line at about 25 words. Do not invent a compressed agent language. The evidence that compression helps comes from trained models, and readability is needed for cross-vendor steps and for the user.
9. **Resume rather than restart where possible.** For fix rounds, resume the implementer session (Claude `--resume`, Codex `codex-reply`/thread id) instead of a new session. This reuses the cached prefix and avoids paying the cold start again.
