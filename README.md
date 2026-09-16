<div align="center">

<img src="src-tauri/icons/icon.png" width="120" alt="Coding Agent History Viewer" />

# Coding Agent History Viewer

**本地、只读地浏览 Claude Code 与 OpenAI Codex 的 Prompt、会话和用量**

**A local, read-only viewer for Claude Code and OpenAI Codex history**

[简体中文](#简体中文) · [English](#english)

</div>

---

## 简体中文

Coding Agent History Viewer 是一个 Tauri 桌面应用。它在本机扫描 Claude Code 与 OpenAI Codex 的历史文件，按工作目录聚合 Prompt、会话、工具调用和 Token 用量。应用不联网，也不会把历史上传到任何服务。

### 功能

- 在概览顶部按 **Claude Code / Codex / 全部** 筛选，默认选择“全部”并在本机持久化选择。
- 按工作目录浏览项目、Prompt 和会话；“全部”模式下相同 `cwd` 合并为一个项目。
- 查看最近 Prompt、全局搜索、文件夹内搜索、完整会话、Markdown、工具调用和思考内容。
- **全文搜索会话内容**：除 Prompt 外，还能按需扫描助手回复、思考摘要与工具调用参数，命中可跳转到对话中的具体消息并高亮关键词；不建索引、不落盘。
- **对话详情**：助手回复按 Markdown 渲染并高亮代码；Bash/Edit/Write/Read/TodoWrite 与 Codex apply_patch 等工具调用按语义展示（命令、行级 diff、补丁、清单）；会话内查找、用户轮次大纲、全部展开/折叠、超长会话分批渲染。
- **每会话 Token 与成本**：会话列表可按最新 / 成本 / 消息数 / 时长排序；fork/resume 复制的调用只计入原会话，各会话之和等于全局总量。
- **批量导出会话**：在文件夹页的「会话」标签页勾选多个会话一次导出，可合成一份 Markdown（带目录表），或每个会话一份并附 index.md；可选是否包含工具调用与思考过程。单会话与批量导出都走不截断的解析路径，超长工具结果完整保留。
- **统计范围与活跃度日历**：概览支持全部 / 近 7 天 / 近 30 天 / 本月 / 自定义区间，日历热力图补齐没有记录的日期。
- Prompt、会话和导出内容保留 Claude Code 或 Codex 来源标识。
- 统计每日活动、小时与星期分布、项目排行、模型、CLI 版本、Token、缓存命中率和估算成本。
- 按文件 `mtime` 建立增量索引，并行扫描大历史库；JSONL 按行流式解析。索引在后台线程构建并实时显示进度，刷新默认只重解析变化的文件，全量重建放在设置中。
- 搜索是独立路由，返回键可回到结果列表；记住窗口位置与尺寸；主题可跟随系统。
- 任一产品的数据目录不存在时，仍可正常浏览另一个产品的数据。

### 快捷键

| 快捷键 | 作用 |
|---|---|
| ⌘/Ctrl + K | 聚焦搜索框 |
| Esc | 清空搜索并返回上一页 / 关闭设置 |
| ⌘/Ctrl + R、F5 | 增量刷新本地数据 |
| ⌘/Ctrl + , | 打开设置 |
| ⌘/Ctrl + F（对话详情内） | 会话内查找，Enter / Shift+Enter 切换命中 |

### 数据路径

#### Claude Code

| 数据 | 默认路径 | 用途 |
|---|---|---|
| Prompt 历史 | `~/.claude/history.jsonl` | 输入框历史 |
| 完整会话 | `~/.claude/projects/**/*.jsonl` | 用户/助手消息、工具调用和用量 |
| 会话元数据 | `~/.claude/sessions/*.json` | CLI 版本等补充信息 |

设置中的 `historyFile`、`projectsDir`、`sessionsDir` 可分别覆盖路径；否则从 `claudeDataDir` 推导；仍未配置时使用 `~/.claude`。这些旧字段继续兼容已有 `settings.json`。

#### OpenAI Codex

| 数据 | 默认路径 | 用途 |
|---|---|---|
| Prompt 历史 | `<CODEX_ROOT>/history.jsonl` | `session_id`、`text`、`ts` |
| 当前会话 | `<CODEX_ROOT>/sessions/YYYY/MM/DD/rollout-*.jsonl` | 完整事件流 |
| 归档会话 | `<CODEX_ROOT>/archived_sessions/*.jsonl` | 已归档事件流 |

`CODEX_ROOT` 的优先级固定为：

1. 设置中的 `codexDataDir`
2. 环境变量 `CODEX_HOME`
3. `~/.codex`

设置页会分别显示两种产品的配置值、最终解析路径和存在状态。保存设置后重新建立索引，无需重新编译。旧版仅含 Claude 字段的设置文件会自动按默认值补齐 Codex 配置。

设置文件位于系统应用配置目录。macOS 默认是：

```text
~/Library/Application Support/com.xzl.cchistoryviewer/settings.json
```

为兼容旧开发环境，源码根目录的 `settings.json` 仍可作为低优先级回退。示例见 [`settings.example.json`](./settings.example.json)。bundle identifier `com.xzl.cchistoryviewer` 保持不变，因此原有应用配置目录不会丢失。

### Codex 格式兼容

- 从 `session_meta` 读取会话 ID、`cwd`、CLI 版本和客户端来源，从最近的 `turn_context` 读取模型。
- 从 `event_msg.user_message` 提取当前格式的真实用户 Prompt；`history.jsonl` 通过 `session_id` 关联 `session_meta.cwd`。
- 完全旧版 rollout 缺少 `event_msg.user_message` 时，回退到 `response_item` 的 `role=user`；若同一文件中途升级格式，则保留首个 event 之前清理后的旧 Prompt，并从首个 event 起只信任 `event_msg.user_message`。developer、system、`AGENTS.md`、`environment_context`、`codex_internal_context` 等注入内容不会成为 Prompt。
- 从 `response_item` 提取助手消息、函数或自定义工具调用及结果；未知事件会被忽略。
- 旧 rollout 可以只有 `session_meta` 和 `response_item`，缺少模型与 Token 时仍可浏览。
- `history.jsonl` 的 `ts` 同时接受 Unix 秒和毫秒。单行 JSON 损坏不会使整个文件或索引失败。
- 默认 Prompt 与会话统计排除自动生成的 sub-agent Prompt；sub-agent 的真实模型调用仍计入 Token，并归属其自身 `cwd`。

Codex 文件已可能超过 1 GB。解析器逐行读取，不会先把整个 JSONL 加载进内存；会话文件仍按文件并行扫描。

### 统一模型与筛选口径

公共领域模型使用：

- `agent`: `claude | codex`
- `origin`: `history | conversation | both`

`origin` 只描述 Prompt 来自输入历史、会话文件或两者，不再与产品来源混用。Prompt、项目、会话、消息、用量和缓存记录均保留 `agent`。稳定 ID、会话映射、路由、去重键和缓存键都包含 `agent`，因此两个产品即使出现相同 `session_id` 也不会冲突；跨产品的相同文本不会互相去重。

“全部”筛选满足以下恒等关系：

- Prompt 数、会话数和每个归一化 Token 分项等于 Claude 与 Codex 之和。
- 项目数等于两个产品 `cwd` 路径的并集大小，不是两个项目数简单相加。
- 相同 `cwd` 的项目在导航中合并，但其 Prompt 和会话仍保留 `agent`。

### Token 统计

统一用量字段如下：

| 字段 | 含义 |
|---|---|
| `uncachedInput` | 未命中缓存的输入 Token |
| `cacheRead` | 从缓存读取的输入 Token |
| `cacheCreation` | 创建缓存使用的输入 Token |
| `output` | 输出 Token，包含 reasoning 输出 |
| `reasoningOutput` | `output` 的拆分子集，仅展示 |

总 Token 开销（含缓存）统一计算为：

```text
totalTokensIncludingCache = uncachedInput + cacheRead + cacheCreation + output
```

`reasoningOutput` 已包含在 `output` 中，绝不再次加入总量。

Claude Code 映射：

```text
uncachedInput = input_tokens
cacheRead = cache_read_input_tokens
cacheCreation = cache_creation_input_tokens
output = output_tokens
```

Codex 映射只使用每个 `token_count.info.last_token_usage`，不累加累计字段 `total_token_usage`：

```text
uncachedInput = input_tokens - cached_input_tokens
cacheRead = cached_input_tokens
cacheCreation = 0
output = output_tokens
reasoningOutput = reasoning_output_tokens
```

Codex 的 `input_tokens` 已包含 `cached_input_tokens`，所以不能再把两者作为两个完整输入量相加。fork、resume 或派生会话可能复制历史 Token 事件；索引使用不依赖文件路径和新会话 ID 的稳定事件指纹跨文件去重。

缓存命中率为：

```text
cacheRead / (uncachedInput + cacheRead)
```

分母为 0 时显示“—”。概览、按天、按模型和按项目的总 Token 均使用同一公式。

每个会话的 Token 与成本按同一去重规则归属：按会话开始时间顺序，一条调用首次出现在哪个会话就计入哪个会话，fork/resume 复制的历史调用只计入原会话；Claude 子代理的调用并入父会话。因此各会话之和等于全局总量。

### 成本说明

成本按产品和可靠匹配的具体模型分别估算。Codex 成本显示为 **API 等价估算**，仅表示按对应 OpenAI API 标准 Token 单价换算的参考值，不代表 ChatGPT 或 Codex 订阅的实际账单、额度或积分消耗。

无法可靠匹配价格的模型显示“—”。合并统计只累加已知价格的估算成本，并显示未知价格 Token 的覆盖提示；不会把未知价格当作零成本。内置价格于 2026-07-17 对照 [Anthropic 定价](https://platform.claude.com/docs/en/about-claude/pricing) 与 OpenAI 官方模型页（例如 [GPT-5.5](https://developers.openai.com/api/docs/models/gpt-5.5)、[GPT-5.4](https://developers.openai.com/api/docs/models/gpt-5.4)、[GPT-5.4 mini](https://developers.openai.com/api/docs/models/gpt-5.4-mini)、[GPT-5.1 Codex Max](https://developers.openai.com/api/docs/models/gpt-5.1-codex-max)、[o4-mini](https://developers.openai.com/api/docs/models/o4-mini) 与 [GPT-4.1](https://developers.openai.com/api/docs/models/gpt-4.1)）复核；模型价格变化后应重新核验。

### 增量缓存

索引缓存 schema 为 **v5**。每条 history/会话文件缓存记录包含 `agent`，缓存键包含产品和文件身份；文件 `mtime` 与长度指纹未变化时复用解析结果，仅重新解析新增或变化的文件。删除文件、路径设置变化或 cache schema 版本变化会使对应缓存失效。Claude 与 Codex 扫描均可并行进行。

缓存写在本应用的系统数据目录，不写入 `~/.claude` 或 `~/.codex`。缓存是本地派生数据，可能含解析后的 Prompt、消息摘要和用量；它与原始历史一样应按敏感数据保护。

索引在后台线程构建，构建期间界面显示扫描与解析进度，其他查询等待同一次构建完成。顶栏刷新按钮与 ⌘/Ctrl+R 只重解析指纹变化的文件；设置中的“全量重建”忽略缓存重新解析全部文件。重建期间旧索引继续可用。

### 全文搜索

搜索页的“会话内容”模式不建立任何索引：每次搜索并行流式扫描当前筛选范围内的会话文件，先用大小写不敏感的字节子串预筛原始行，只解析命中的行。覆盖用户消息、助手回复、思考摘要与工具调用参数；工具结果正文（文件内容、命令输出）不参与匹配，developer/system 与 `AGENTS.md` 等注入内容也不会命中。多个关键词需同时出现在同一条消息中。每个会话最多返回 20 条、总计最多 300 条，超出时提示缩小范围。

### 隐私边界

- 不读取 Codex `auth.json`。
- 不把 Codex 私有 SQLite 表作为主要数据源。
- 不联网、不上传、不遥测历史内容。
- 不修改或删除 `~/.claude`、`~/.codex` 及自定义数据目录中的任何内容。
- 应用只写自身设置、索引缓存、窗口位置状态，以及用户主动导出的 Markdown 文件；全文搜索按需读取会话文件，不写任何索引。
- 对话里的链接只在用户点击时交给系统浏览器打开，应用本身不发起任何网络请求。

### 开发与验证

要求 Node.js 18+、pnpm 8+ 和 Rust 工具链。Rust 安装见 [`Rust工具链安装指南.md`](./Rust工具链安装指南.md)。

```bash
pnpm install
pnpm tauri dev
```

发布前门禁：

```bash
cd src-tauri
cargo fmt --check
cargo test
cd ..
pnpm exec tsc --noEmit
pnpm build
```

`cargo test` 包含完全合成的 Claude 与 Codex fixture/golden 数据，不应加入任何真实 Prompt、私人绝对路径、凭据或真实缓存。真实数据验收只做只读冒烟检查，不在日志或报告中输出内容。

---

## English

Coding Agent History Viewer is a Tauri desktop application that scans Claude Code and OpenAI Codex history locally and groups prompts, sessions, tool calls, and token usage by working directory. It makes no network requests and does not upload history.

### Features

- Filter the overview by **Claude Code / Codex / All**. All is the default and the choice is persisted locally.
- Browse projects, prompts, and sessions by working directory. In All mode, an identical `cwd` is one project.
- View recent prompts, global and folder search, full conversations, Markdown, tool calls, and thinking content.
- **Full-text conversation search**: beyond prompts, scan assistant replies, thinking summaries, and tool-call inputs on demand; hits jump to the exact message with keywords highlighted. No index is built or written.
- **Conversation view**: assistant replies render as Markdown with syntax-highlighted code; Bash/Edit/Write/Read/TodoWrite and Codex apply_patch calls render semantically (commands, line diffs, patches, checklists); in-conversation find, a user-turn outline, expand/collapse all, and batched rendering for very long sessions.
- **Per-session tokens and cost**: sort sessions by newest, cost, messages, or duration; calls copied by fork/resume count only in the original session, so session totals add up to the global totals.
- **Batch session export**: tick several sessions on a folder's Sessions tab and export them at once, either as one merged Markdown file with a table of contents or as one file per session plus an index.md; optionally include tool calls and thinking. Single and batch exports parse without the display clip, so long tool results are kept whole.
- **Time range and activity calendar**: scope the overview to all time, last 7 / 30 days, this month, or a custom range; a calendar heatmap fills in days without records.
- Preserve the Claude Code or Codex identity on prompts, sessions, and exports.
- Compare activity, model and CLI versions, normalized tokens, cache hit rate, and estimated cost.
- Stream JSONL line by line, scan files in parallel, and reuse a per-file `mtime` cache. The index builds on a background thread with live progress; refresh re-parses only changed files, and a full rebuild lives in Settings.
- Search is its own route so the back button returns to results; window position and size are remembered; the theme can follow the system.
- Continue working when either product's data directory is absent.

### Keyboard shortcuts

| Shortcut | Action |
|---|---|
| ⌘/Ctrl + K | Focus the search box |
| Esc | Clear the search and go back / close Settings |
| ⌘/Ctrl + R, F5 | Incrementally refresh local data |
| ⌘/Ctrl + , | Open Settings |
| ⌘/Ctrl + F (in a conversation) | Find in conversation; Enter / Shift+Enter move between hits |

### Data paths and precedence

Claude Code defaults to `~/.claude/history.jsonl`, `~/.claude/projects/**/*.jsonl`, and `~/.claude/sessions/*.json`. Legacy `historyFile`, `projectsDir`, and `sessionsDir` settings override individual paths; otherwise paths derive from `claudeDataDir`, then `~/.claude`.

Codex reads:

```text
<CODEX_ROOT>/history.jsonl
<CODEX_ROOT>/sessions/YYYY/MM/DD/rollout-*.jsonl
<CODEX_ROOT>/archived_sessions/*.jsonl
```

`CODEX_ROOT` precedence is explicit `codexDataDir`, then `CODEX_HOME`, then `~/.codex`. The settings view reports configured and resolved paths separately for both products. Existing Claude-only settings remain valid. The bundle identifier stays `com.xzl.cchistoryviewer` so existing application settings are retained.

### Codex compatibility

- `session_meta` supplies session ID, `cwd`, CLI version, and client source; the latest `turn_context` supplies the active model.
- Current user prompts come from `history.jsonl` and `event_msg.user_message`, associated through `session_id` and `session_meta.cwd`.
- A fully legacy rollout falls back to `response_item` records with `role=user`. If one file changes format mid-stream, sanitized legacy prompts before the first event are retained, while `event_msg.user_message` becomes canonical from that point onward. Developer/system messages and injected `AGENTS.md`, `environment_context`, or `codex_internal_context` content are excluded.
- Assistant messages and function/custom tool calls come from `response_item`. Unknown events are ignored.
- Old rollouts containing only `session_meta` and `response_item` remain readable without model or token data.
- The `ts` field in `history.jsonl` accepts Unix seconds or milliseconds. One malformed JSONL line does not abort the file or index.
- Automatic subagent prompts and sessions are excluded from default prompt/session statistics, while their actual model calls remain in token usage under the subagent `cwd`.

Codex JSONL is streamed rather than loaded as a whole, including histories larger than 1 GB, while files continue to be scanned in parallel.

### Domain and filtering invariants

The normalized model uses `agent = claude | codex` and `origin = history | conversation | both`. `origin` describes where a prompt was observed, not which product produced it. Agent identity is part of every stable ID, session route, deduplication key, and cache key. Equal session IDs or prompt text from different products never collide or deduplicate.

For the All filter, prompt count, session count, and every normalized token component equal Claude plus Codex. Project count is the set union of `cwd` paths. Merged project navigation never removes the agent identity from its prompts and sessions.

### Token accounting

The unified fields are `uncachedInput`, `cacheRead`, `cacheCreation`, `output`, and `reasoningOutput`.

```text
totalTokensIncludingCache = uncachedInput + cacheRead + cacheCreation + output
```

`reasoningOutput` is a subset of `output` and is not added again.

Claude maps `input_tokens`, `cache_read_input_tokens`, `cache_creation_input_tokens`, and `output_tokens` directly to the first four fields.

For Codex, only each `token_count.info.last_token_usage` is added; cumulative `total_token_usage` is ignored:

```text
uncachedInput = input_tokens - cached_input_tokens
cacheRead = cached_input_tokens
cacheCreation = 0
output = output_tokens
reasoningOutput = reasoning_output_tokens
```

Codex `input_tokens` already includes cached input. Copied fork/resume usage is removed with a stable event fingerprint independent of file path and a new session ID. Cache hit rate is `cacheRead / (uncachedInput + cacheRead)` and displays “—” for a zero denominator.

Per-session usage follows the same deduplication: sessions are visited in start-time order and each call counts in the first session that recorded it, so fork/resume copies stay with the original session and Claude sub-agent calls roll up into their parent session. Session totals therefore add up to the global totals.

### Cost, cache, and privacy

Codex cost is labelled an **API-equivalent estimate**. It is a reference conversion using reliably matched OpenAI API token prices, not an actual ChatGPT/Codex subscription charge, allowance, or credit balance. Unknown models display “—”; combined totals include known estimated cost and disclose how many tokens have unknown pricing. Built-in rates were checked on 2026-07-17 against [Anthropic pricing](https://platform.claude.com/docs/en/about-claude/pricing) and official OpenAI model pages such as [GPT-5.5](https://developers.openai.com/api/docs/models/gpt-5.5), [GPT-5.4](https://developers.openai.com/api/docs/models/gpt-5.4), [GPT-5.4 mini](https://developers.openai.com/api/docs/models/gpt-5.4-mini), [GPT-5.1 Codex Max](https://developers.openai.com/api/docs/models/gpt-5.1-codex-max), [o4-mini](https://developers.openai.com/api/docs/models/o4-mini), and [GPT-4.1](https://developers.openai.com/api/docs/models/gpt-4.1).

Cache schema **v5** is agent-aware and stores per-file parsed results keyed by product and file identity. Unchanged `mtime` and file-length fingerprints are reused; changed, added, removed, reconfigured, or old-schema entries are rebuilt. The cache lives in this application's data directory and may contain derived prompt text, summaries, and usage.

The index builds on a background thread and reports progress while other queries wait for the same build. The toolbar refresh and ⌘/Ctrl+R re-parse only files whose fingerprint changed; the full rebuild in Settings ignores the cache. The previous index stays available while a rebuild runs.

Full-text conversation search builds no index: each search streams the session files in the current scope in parallel, prefilters raw lines with a case-insensitive byte search, and parses only matching lines. It covers user messages, assistant replies, thinking summaries, and tool-call inputs; tool results (file contents, command output) and injected developer/system or `AGENTS.md` context never match. Every keyword must appear in the same message. At most 20 hits per session and 300 in total are returned, with a notice when results are truncated.

The application does not read Codex `auth.json`, does not use private Codex SQLite tables as its primary source, makes no history upload or telemetry request, and never writes or deletes data under Claude/Codex roots. It writes only its own settings, index cache, window state, and Markdown files explicitly exported by the user. Links inside conversations open in the system browser only when clicked.

### Development and release checks

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

### License

[MIT](./LICENSE)
