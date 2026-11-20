<div align="center">

<img src="src-tauri/icons/icon.png" width="120" alt="Coding Agent History Viewer" />

# Coding Agent History Viewer

**A local, read-only viewer for Claude Code, OpenAI Codex and Antigravity history**

English · [简体中文](./README.zh-CN.md)

</div>

---

Coding Agent History Viewer is a Tauri desktop application that scans Claude Code, OpenAI Codex and Antigravity (Antigravity CLI plus the Gemini CLI it replaced) history locally and groups prompts, sessions, tool calls, and token usage by working directory. It only contacts GitHub to check for and download updates, and never uploads history.

## Features

- Filter the overview by **Claude Code / Codex / Antigravity / All**. All is the default and the choice is persisted locally.
- Browse projects, prompts, and sessions by working directory. In All mode, an identical `cwd` is one project.
- View recent prompts, global and folder search, full conversations, Markdown, tool calls, and thinking content.
- **Full-text conversation search**: beyond prompts, scan assistant replies, thinking summaries, and tool-call inputs on demand; hits jump to the exact message with keywords highlighted. No index is built or written.
- **Conversation view**: assistant replies, thinking and sub-agent reports / plans render as Markdown with syntax-highlighted code; bold and strikethrough next to CJK punctuation and single line breaks show as intended; Codex follow-up suggestions appear as "↪" chips and links to local files show their path on hover. A "Markdown / Raw" switch at the top is remembered; user messages and command output always stay raw. Bash/Edit/Write/Read/TodoWrite and Codex apply_patch calls render semantically (commands, line diffs, patches, checklists); in-conversation find, a user-turn outline (plain-text previews), expand/collapse all, and batched rendering for very long sessions.
- **Per-session tokens and cost**: sort sessions by newest, cost, messages, or duration; calls copied by fork/resume count only in the original session, so session totals add up to the global totals.
- **In-session token usage details**: the conversation view has a collapsible "Token usage details" panel: five stat tiles (total / cost / input / output / cache hit rate, with average context per call, average cost per turn, sub-agent share, the 5-minute / 1-hour cache-write split and the saving versus no caching), token and cost mix bars, a stacked per-call chart in call order (click a bar to jump to the message) and, for sessions with several models or reasoning efforts, a per-model table with one row per effort; the output tile and the tooltips show the reasoning effort. Every assistant message carries a context / output / cost chip for its call, and the outline shows tokens and cost per user turn.
- **Batch session export**: tick several sessions on a folder's Sessions tab and export them at once, either as one merged Markdown file with a table of contents or as one file per session plus an index.md; optionally include tool calls and thinking. Single and batch exports parse without the display clip, so long tool results are kept whole.
- **Import Claude Code sessions**: bring a zip packed by Claude Code on the web (claude.ai/code) into the local projects directory, then browse, search and export those sessions like local ones. A read-only plan classifies every session as add / update / skip / conflict first; conflicts are resolved one by one with "keep local" or "overwrite with import"; the cloud `/home/user/<repo>` can be mapped to a local folder and the mapping is remembered. See [Importing Claude Code sessions](#importing-claude-code-sessions).
- **External sessions (read someone else's exported Markdown)**: import a single-conversation Markdown file that someone else exported with this app from the sidebar's "External sessions" page and read it in the same conversation view. The detailed and brief exports of one session merge into one entry with a "Full / Dialogue only" switch. Imports last only for the current run unless you click "Keep", which copies them into the app's data directory; nothing is written to `~/.claude` or `~/.codex`, and external sessions stay out of stats and search. See [External sessions](#external-sessions).
- **Conversation view additions**: tool outputs that Claude Code persisted to a file (`<persisted-output>`) can be expanded in full on demand and are inlined in exports; files referenced with @ or uploaded show as a collapsed "Attachment" block; thinking blocks that carry only a signature are hidden.
- **Time range and activity calendar**: scope the overview to all time, last 7 / 30 days, this month, or a custom range; a calendar heatmap fills in days without records.
- Preserve the Claude Code, Codex or Antigravity identity on prompts, sessions, and exports.
- Compare activity, model and CLI versions, normalized tokens, cache hit rate, and estimated cost.
- Stream JSONL line by line, scan files in parallel, and reuse a per-file `mtime` cache. The index builds on a background thread with live progress; refresh re-parses only changed files, and a full rebuild lives in Settings.
- Search is its own route so the back button returns to results; window position and size are remembered; drag the sidebar's right edge to widen it (220–520 px, remembered; double-click resets), and from about 330 px the four source filters fit on one row; the theme can follow the system.
- **In-app updates**: checks GitHub for a newer version at startup and while the app is open (at most once an hour; can be turned off in Settings) and shows it in the toolbar; "Update now" downloads it, verifies its signature, installs it and reopens the app. See [In-app updates](#in-app-updates).
- Continue working when either product's data directory is absent.

## Keyboard shortcuts

| Shortcut | Action |
|---|---|
| ⌘/Ctrl + K | Focus the search box |
| Esc | Clear the search and go back / close Settings |
| ⌘/Ctrl + R, F5 | Incrementally refresh local data |
| ⌘/Ctrl + , | Open Settings |
| ⌘/Ctrl + F (in a conversation) | Find in conversation; Enter / Shift+Enter move between hits |

## Data paths and precedence

Claude Code defaults to `~/.claude/history.jsonl`, `~/.claude/projects/**/*.jsonl`, and `~/.claude/sessions/*.json`. Legacy `historyFile`, `projectsDir`, and `sessionsDir` settings override individual paths; otherwise paths derive from `claudeDataDir`, then `~/.claude`. `importPathMappings` stores the cloud-to-local folder pairs confirmed during session imports; they can be removed in Settings.

Codex reads:

```text
<CODEX_ROOT>/history.jsonl
<CODEX_ROOT>/sessions/YYYY/MM/DD/rollout-*.jsonl
<CODEX_ROOT>/archived_sessions/*.jsonl
```

`CODEX_ROOT` precedence is explicit `codexDataDir`, then `CODEX_HOME`, then `~/.codex`.

Antigravity reads below `<GEMINI_ROOT>`, the explicit `geminiDataDir` or else `~/.gemini`, which Antigravity CLI and Gemini CLI share:

```text
<GEMINI_ROOT>/antigravity-cli/history.jsonl
<GEMINI_ROOT>/antigravity-cli/brain/<conversation id>/.system_generated/logs/transcript_full.jsonl   (else transcript.jsonl)
<GEMINI_ROOT>/antigravity-cli/cache/last_conversations.json
<GEMINI_ROOT>/tmp/<project>/chats/session-*.jsonl | session-*.json
<GEMINI_ROOT>/tmp/<project>/chats/<parent session id>/<sub-agent session id>.jsonl
<GEMINI_ROOT>/tmp/<project>/.project_root, <GEMINI_ROOT>/projects.json
```

The settings view reports configured and resolved paths separately for each product. Existing Claude-only settings remain valid. The bundle identifier stays `com.xzl.cchistoryviewer` so existing application settings are retained.

## Codex compatibility

- `session_meta` supplies session ID, `cwd`, CLI version, and client source; the latest `turn_context` supplies the active model.
- Current user prompts come from `history.jsonl` and `event_msg.user_message`, associated through `session_id` and `session_meta.cwd`.
- A fully legacy rollout falls back to `response_item` records with `role=user`. If one file changes format mid-stream, sanitized legacy prompts before the first event are retained, while `event_msg.user_message` becomes canonical from that point onward. Developer/system messages and injected `AGENTS.md`, `environment_context`, or `codex_internal_context` content are excluded.
- Assistant messages and function/custom tool calls come from `response_item`. Unknown events are ignored.
- Old rollouts containing only `session_meta` and `response_item` remain readable without model or token data.
- The `ts` field in `history.jsonl` accepts Unix seconds or milliseconds. One malformed JSONL line does not abort the file or index.
- Automatic subagent prompts and sessions are excluded from default prompt/session statistics, while their actual model calls remain in token usage under the subagent `cwd`.

Codex JSONL is streamed rather than loaded as a whole, including histories larger than 1 GB, while files continue to be scanned in parallel.

## Antigravity compatibility

Antigravity covers Antigravity CLI (`agy`) and the Gemini CLI it replaced; the session's source reads `antigravity-cli` or `gemini-cli`, and the resume command follows it (`agy --conversation <id>` / `gemini --resume <id>`). Only plain-text files are read: the Antigravity IDE and desktop app, and the CLI's SQLite and protobuf files, are out of scope. See the [Agent Note](.agents/notes/proposed/2026-11-20-antigravity-source.md).

- **Antigravity CLI**: each line of the transcript is one step. `USER_INPUT` is the prompt (only the `<USER_REQUEST>` block; the injected local time and model-change notes are dropped), `PLANNER_RESPONSE` is one model call with its reply, thinking, tool calls, model and tokens, and the `GENERIC` steps after it are the tool results, paired with the calls in order. Reminders in `SYSTEM_MESSAGE` are hidden; a report a sub-agent sends back shows as a `subagent` result. `transcript_full.jsonl` keeps tool arguments typed; `transcript.jsonl`, which JSON-encodes every argument value, is used only when the full one is missing.
- **Working directory**: transcripts carry none. It comes from `history.jsonl` (by conversation ID, else a line with the same prompt text within five minutes, since a conversation's first input is logged before it has an ID), then `cache/last_conversations.json`; a sub-agent inherits its parent's. A conversation found nowhere (some `agy -p` runs) has no folder.
- **Sub-agents**: an Antigravity CLI conversation that opens with a message from another conversation, and a Gemini CLI session with `kind: subagent`, are sub-agents. They are not listed as sessions or prompts; their calls roll up into the parent session.
- **Gemini CLI**: sessions are replayed the way Gemini CLI loads them: the header line, message lines (same ID replaces), `$set` (a `messages` array replaces all), `$patch` (content, tool results, removals, reordering) and `$rewindTo`; an older `session-*.json` is one whole document. Injected `<session_context>` / `<hook_context>` turns are not prompts. The project path comes from `.project_root` or `projects.json`; a directory still named by the SHA-256 of the path is matched against every known working directory.

## Domain and filtering invariants

The normalized model uses `agent = claude | codex | antigravity` and `origin = history | conversation | both`. `origin` describes where a prompt was observed, not which product produced it. Agent identity is part of every stable ID, session route, deduplication key, and cache key. Equal session IDs or prompt text from different products never collide or deduplicate.

For the All filter, prompt count, session count, and every normalized token component equal the sum of the three products. Project count is the set union of `cwd` paths. Merged project navigation never removes the agent identity from its prompts and sessions.

## Token accounting

The unified fields are `uncachedInput`, `cacheRead`, `cacheCreation`, `cacheCreation1h`, `output`, and `reasoningOutput`.

```text
totalTokensIncludingCache = uncachedInput + cacheRead + cacheCreation + output
```

`reasoningOutput` is a subset of `output`, and `cacheCreation1h` (the part of `cacheCreation` billed at the 1-hour tier, Claude only) a subset of `cacheCreation`; neither is added again.

Claude maps `input_tokens`, `cache_read_input_tokens`, `cache_creation_input_tokens`, and `output_tokens` directly to the first four fields; `cacheCreation1h` comes from `cache_creation.ephemeral_1h_input_tokens` (0 when absent, clamped to `cacheCreation`).

For Codex, only each `token_count.info.last_token_usage` is added; cumulative `total_token_usage` is ignored:

```text
uncachedInput = input_tokens - cached_input_tokens
cacheRead = cached_input_tokens
cacheCreation = 0
cacheCreation1h = 0
output = output_tokens
reasoningOutput = reasoning_output_tokens
```

Antigravity CLI `input_tokens` excludes cache reads, and `output_tokens` already includes thinking without splitting it out:

```text
uncachedInput = input_tokens
cacheRead = cache_read_tokens
output = output_tokens
reasoningOutput = 0
```

Gemini CLI `tokens.input` includes the cached part, and thoughts and tool-use prompt tokens are counted beside it (`total = input + output + thoughts + tool`):

```text
uncachedInput = input - cached + tool
cacheRead = cached
output = output + thoughts
reasoningOutput = thoughts
```

Codex `input_tokens` already includes cached input. Copied fork/resume usage is removed with a stable event fingerprint independent of file path and a new session ID. Cache hit rate is `cacheRead / (uncachedInput + cacheRead)` and displays “—” for a zero denominator.

Per-session usage follows the same deduplication: sessions are visited in start-time order and each call counts in the first session that recorded it, so fork/resume copies stay with the original session and sub-agent calls roll up into their parent session. Session totals therefore add up to the global totals.

The conversation view's "Token usage details" panel shares that attribution: the index remembers which session owns each unique call, the detail lists every call the session owns (sub-agent transcripts included), and the per-call and per-model token sums equal the session total in the header. The chip on an assistant message comes from that record itself: a Claude call is written as one record per content block and only the first carries usage; a Codex `token_count` event lands on the nearest earlier model-output message (tool results excluded). Calls copied in by resume/fork that belong to another session are greyed out and excluded from every number. Cache saving = cost with every input token at the uncached rate − estimated cost, negative when cache writes outweigh reads.

The panel also shows each call's reasoning effort as recorded (`low` / `medium` / `high` / `xhigh` / `max`, plus `minimal` for Codex): Claude Code's top-level `perTurnEffort` on an assistant record, falling back to `effort`; Codex's `turn_context.effort`, shared by every call in that turn. Records from older versions carry no effort and show "not recorded"; no default is guessed. Antigravity records no effort field either: the level is part of the model name (`gemini-3.8-flash-low`), which is shown as recorded. The effort appears in the message chip and call chart tooltips and on the output tile (with per-effort call counts, sub-agents included, when a session mixes efforts), and the per-model table splits each model by effort. Effort does not enter the cost estimate.

## Cost, cache, and privacy

Codex cost is labelled an **API-equivalent estimate**. It is a reference conversion using reliably matched OpenAI API token prices, not an actual ChatGPT/Codex subscription charge, allowance, or credit balance. Unknown models display “—”; combined totals include known estimated cost and disclose how many tokens have unknown pricing. Antigravity models are not priced yet, so their tokens are counted and their cost shows “—”. Built-in rates were checked on 2026-07-17 against [Anthropic pricing](https://platform.claude.com/docs/en/about-claude/pricing) and official OpenAI model pages such as [GPT-5.5](https://developers.openai.com/api/docs/models/gpt-5.5), [GPT-5.4](https://developers.openai.com/api/docs/models/gpt-5.4), [GPT-5.4 mini](https://developers.openai.com/api/docs/models/gpt-5.4-mini), [GPT-5.1 Codex Max](https://developers.openai.com/api/docs/models/gpt-5.1-codex-max), [o4-mini](https://developers.openai.com/api/docs/models/o4-mini), and [GPT-4.1](https://developers.openai.com/api/docs/models/gpt-4.1).

Claude cache writes are priced by tier: 5-minute at 1.25× the input rate, 1-hour at 2×; records without the split count as 5-minute.

Cache schema **v7** (v7 adds the reasoning effort to every call, so the first launch after upgrading rebuilds the index once) is agent-aware and stores per-file parsed results keyed by product and file identity. Unchanged `mtime` and file-length fingerprints are reused; changed, added, removed, reconfigured, or old-schema entries are rebuilt. The cache lives in this application's data directory and may contain derived prompt text, summaries, and usage.

The index builds on a background thread and reports progress while other queries wait for the same build. The toolbar refresh and ⌘/Ctrl+R re-parse only files whose fingerprint changed; the full rebuild in Settings ignores the cache. The previous index stays available while a rebuild runs.

Full-text conversation search builds no index: each search streams the session files in the current scope in parallel, prefilters raw lines with a case-insensitive byte search, and parses only matching lines. It covers user messages, assistant replies, thinking summaries, and tool-call inputs; tool results (file contents, command output) and injected developer/system or `AGENTS.md` context never match. Every keyword must appear in the same message. At most 20 hits per session and 300 in total are returned, with a notice when results are truncated.

The application does not read Codex `auth.json`, does not use private Codex SQLite tables as its primary source, reads no Antigravity credentials, SQLite or protobuf files, makes no history upload or telemetry request, never writes under `~/.gemini`, and never deletes data under the Claude/Codex/Gemini roots. The only write into `~/.claude` is a session import the user confirmed, which touches only session files below `projects/<dir>/`, overwrites same-named files and never deletes anything. Beyond that it writes only its own settings, index cache, window state, Markdown files explicitly exported by the user, external-session Markdown the user chose to keep (inside the app's data directory), and its own application files when the user installs an update. Its only network traffic is the update check and download from this repository's GitHub Releases, which carries no history; the automatic check can be turned off in Settings. Links inside conversations open in the system browser only when clicked; remote images in local conversations load directly, while those in external sessions show as links.

## Importing Claude Code sessions

Ask Claude Code on the web to pack the session records as a zip that keeps the `~/.claude` layout, download it, and click **Import** in the top bar:

```text
projects/<project dir>/<session id>.jsonl
projects/<project dir>/<session id>/      ← side directory: tool-results/*.txt, maybe subagents/
```

- **Destination**: the configured Claude projects directory (default `~/.claude/projects`), with the zip's `projects/` prefix removed. Imported sessions are subject to Claude Code's default 30-day cleanup like any local session.
- **Safety**: only entries below `projects/<dir>/` are accepted. Absolute paths, `..` components and symlinks reject the whole archive without writing anything; `__MACOSX/`, `.DS_Store` and other entries outside `projects/` or not belonging to a session are skipped and listed in the summary.
- **Project mapping**: cloud sessions run in `/home/user/<repo>`. The dialog prefills a local project with the same folder name, or lets you pick a folder, or keep the original path. With a mapping, only each line's top-level `cwd` is rewritten, everything else is kept byte for byte, the session lands in the directory that encodes the local path, the viewer groups it with the local project, and `claude --resume <session id>` finds it locally. Confirmed mappings are stored in settings and prefilled next time.
- **Plan rules**: a session is the transcript plus its side directory. Missing locally: add. Local transcript is a prefix of the imported one (a newer snapshot of the same session): update. Imported transcript is a prefix of the local one: skip. Identical: skip, only adding side files the archive has and the local copy lacks. Anything else, including identical transcripts with a differing same-named side file: conflict. Runtime markers such as `ccr-tip.json` are not compared.
- **Conflicts**: the dialog shows the session ID, project, and each side's line count and last record time; choose "keep local" or "overwrite with import" per session. The choice covers the whole session, and cancelling writes nothing.
- **Write and summary**: the plan is computed first and re-validated right before writing; files are written atomically, same-named files are overwritten, nothing is deleted. The index then refreshes incrementally and the summary reports added, updated, skipped, kept-local and overwritten sessions.

## External sessions

A single-conversation Markdown file that someone else exported with this app (the conversation view's "Export Markdown") can be read through **Import Markdown** on the sidebar's "External sessions" page; several files can be picked at once. They never become local sessions.

- **Accepted files**: single-conversation exports (titled "# Conversation Export" or "# 对话导出"), with English or Chinese labels, from Claude Code, Codex or Antigravity. The per-session files of a multi-file batch export use the same format and can be imported too; a merged batch export, index.md, prompt exports and search-result exports are rejected with a reason.
- **Detailed and brief**: files merge by product + session ID. A file with thinking, tool calls, tool results or attachments is the detailed version, anything else the brief one; importing the same version again replaces it. "Full" shows the detailed version; "Dialogue only" shows the brief version, or, without one, the detailed version minus thinking, tool calls, tool results and attachments (the same rule as exporting with "Include tool calls & thinking" unticked). With only the brief version, only the dialogue is available.
- **Fidelity**: messages show the minute-level time printed in the file; several text blocks of one message merge into one; tool-call inputs are restored as JSON and render semantically; tool results name their tool (inferred from call order for older exports); tool results and attachments keep the exported full text without the display clip used for local sessions. Exports carry no per-call usage, so the token usage panel is hidden and only the file header's token total and estimated cost are shown.
- **Temporary or kept**: imports live in memory and disappear when the app quits. "Keep" copies the original Markdown into `external_sessions/<agent>-<session id>/` under the app's data directory and loads it on the next launch; importing a new version of a kept session writes it there too. "Delete" removes the entry and, for a kept one, that directory.
- **Isolation**: external sessions are never written to `~/.claude` or `~/.codex`, never appear in the project list, overview stats, prompt search, full-text search or token totals, and offer no re-export or resume command. Remote images referenced in their Markdown are never loaded; they show as "Remote image: …" links that open in the system browser only when clicked, so the sender cannot learn from an image request that you opened the file.

## In-app updates

The app reads `latest.json` from this repository's latest Release and offers an update only when its version is higher. See the [Agent Note](.agents/notes/implemented/2026-10-11-in-app-updates.md) for the design decision.

- **Checking**: automatic at startup, when the window regains focus and periodically while the app is open, at most once an hour; a failed check counts too, so it is retried an hour later. While a newer version found earlier is still not installed, every launch checks again so the notice stays. A failed check shows no popup; the reason appears under "Updates" in Settings, where automatic checks can be turned off and a check can be run at any time. See the [Agent Note on check timing](.agents/notes/proposed/2026-11-20-update-check-timing.md).
- **Installing**: open the toolbar's update button to read the release notes, then click "Update now". The package's signature is verified against the public key built into the app before anything is installed. macOS replaces the app in place and reopens it; on Windows the installer runs with a progress bar only and reopens the app (the MSI asks for administrator approval once). Settings, the index cache and kept external sessions live in the app's data directory and survive updates.
- **Network**: requests follow the system proxy. If GitHub cannot be reached, "Open download page" in Settings opens the Release page in the browser.
- **First time**: v0.12.0 and earlier lack this feature; install v0.13.0 or later manually once.

## Persisted tool outputs and attachments

Claude Code stores very large tool outputs in `projects/<dir>/<session id>/tool-results/*.txt` and leaves a `<persisted-output>` block with the absolute path of the machine that produced it. The conversation view re-resolves that path under the current projects directory, first through the session's own directory and then through the part after `projects/`, accepting both `/` and `\`; when the file exists a "View full output" button appears, otherwise the original preview stays. Exports that include tool calls inline the full output and note its source file. Transcripts are never rewritten.

Files referenced with @ or uploaded are recorded as `attachment` lines; they render as a collapsed "Attachment: file name" block right after the user message and are exported when tool calls are included. Thinking blocks from cloud sessions carry only a signature with an empty body; such blocks are hidden.

## Development and release checks

```bash
pnpm install
pnpm tauri dev

cd src-tauri
cargo fmt --check
cargo test
cd ..
pnpm exec tsc --noEmit
pnpm build
```

Tests use synthetic fixtures only. Real Claude/Codex data is allowed solely for a read-only smoke test and must never be committed or printed in a report.

## License

[MIT](./LICENSE)
