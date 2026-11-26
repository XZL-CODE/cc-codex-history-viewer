<div align="center">

<img src="src-tauri/icons/icon.png" width="120" alt="Coding Agent History Viewer" />

# Coding Agent History Viewer

**本地、只读地浏览 Claude Code、OpenAI Codex 与 Antigravity 的 Prompt、会话和用量**

[English](./README.md) · 简体中文

</div>

---

Coding Agent History Viewer 是一个 Tauri 桌面应用。它在本机扫描 Claude Code、OpenAI Codex 与 Antigravity（Antigravity CLI 及它接替的 Gemini CLI）的历史文件，按工作目录聚合 Prompt、会话、工具调用和 Token 用量。应用只在检查和下载更新时访问 GitHub，不会把历史上传到任何服务。

## 功能

- 在概览顶部按 **Claude Code / Codex / Antigravity / 全部** 筛选，默认选择“全部”并在本机持久化选择。
- 按工作目录浏览项目、Prompt 和会话；“全部”模式下相同 `cwd` 合并为一个项目。
- 查看最近 Prompt、全局搜索、文件夹内搜索、完整会话、Markdown、工具调用和思考内容。
- **全文搜索会话内容**：除 Prompt 外，还能按需扫描助手回复、思考摘要与工具调用参数，命中可跳转到对话中的具体消息并高亮关键词；不建索引、不落盘。
- **对话详情**：助手回复、思考与子代理报告 / 计划按 Markdown 渲染并高亮代码，中文标点旁的粗体与删除线、段内换行都按原意显示；Codex 的建议追问显示为「↪」小标签，指向本地文件的链接悬停显示路径。顶部可切换「Markdown / 原文」，选择会被记住；用户消息与命令输出始终显示原文。Bash/Edit/Write/Read/TodoWrite 与 Codex apply_patch 等工具调用按语义展示（命令、行级 diff、补丁、清单）；会话内查找、用户轮次大纲（预览为去掉标记的纯文本）、全部展开/折叠、超长会话分批渲染。渲染规则见 [Agent Note：对话 Markdown 的渲染规则](.agents/notes/implemented/2026-10-11-conversation-markdown-rendering.md)。
- **每会话 Token 与成本**：会话列表可按最新 / 成本 / 消息数 / 时长排序；fork/resume 复制的调用只计入原会话，各会话之和等于全局总量。
- **会话内 Token 用量明细**：对话详情页可展开「Token 用量明细」面板：总量 / 成本 / 输入 / 输出 / 缓存命中率五张指标卡（附平均每次调用上下文、每轮平均成本、子代理占比、缓存 5m / 1h 分档、相比不用缓存节省多少），Token 与成本构成条，按调用顺序的堆叠柱图（点击跳到对应消息），多模型或多推理档位的会话再加按模型拆分表（同一模型按档位分行）；输出卡片与各处悬停显示推理档位；每条助手消息带本次调用的上下文 / 输出 / 成本小标签，大纲里每个用户轮次显示本轮 Token 与成本。
- **批量导出会话**：在文件夹页的「会话」标签页勾选多个会话一次导出，可合成一份 Markdown（带目录表），或每个会话一份并附 index.md；可选是否包含工具调用与思考过程。单会话与批量导出都走不截断的解析路径，超长工具结果完整保留。
- **导入 Claude Code 会话**：把云端 Claude Code（claude.ai/code）打包的 zip 导入本机 projects 目录，之后与本机会话一样检索、查看和导出。导入前先生成计划，逐会话判定新增 / 更新 / 跳过 / 冲突，冲突逐个选择「保留本机」或「用导入的覆盖」；云端的 `/home/user/<仓库名>` 可映射到本机目录，映射会被记住。详见[导入 Claude Code 会话](#导入-claude-code-会话)。
- **外部会话（临时查看他人导出的 Markdown）**：在侧栏「外部会话」导入别人用本应用导出的单会话 Markdown，用同一套对话详情阅读。同一会话的详细版与简洁版合并为一条，可在「完整 / 只看对话」之间切换。默认只在本次运行中保留，点「保留」才复制到应用数据目录；不写入 `~/.claude` 或 `~/.codex`，不计入统计与搜索。详见[外部会话](#外部会话)。
- **对话详情补充**：`<persisted-output>` 标记的超大工具输出可在详情里按需展开完整内容，导出时内联；@ 引用或上传的文件以「附件」折叠块显示；只有 signature、正文为空的思考块不再显示。
- **统计范围与活跃度日历**：概览支持全部 / 近 7 天 / 近 30 天 / 本月 / 自定义区间，日历热力图补齐没有记录的日期。
- Prompt、会话和导出内容保留 Claude Code、Codex 或 Antigravity 来源标识。
- 统计每日活动、小时与星期分布、项目排行、模型、CLI 版本、Token、缓存命中率和估算成本。
- 按文件 `mtime` 建立增量索引，并行扫描大历史库；JSONL 按行流式解析。索引在后台线程构建并实时显示进度，刷新默认只重解析变化的文件，全量重建放在设置中。
- 搜索是独立路由，返回键可回到结果列表；记住窗口位置与尺寸；拖动侧栏右边缘可调宽度（220–520 px，会被记住，双击恢复默认），约 330 px 起四个来源筛选排成一行；主题可跟随系统。
- **应用内更新**：启动后和运行期间每个整点自动检查 GitHub 上的新版本（可在设置中关闭），发现后在顶栏提示；点「立即更新」下载、核对签名后安装并重新打开应用。详见[应用内更新](#应用内更新)。
- 任一产品的数据目录不存在时，仍可正常浏览另一个产品的数据。

## 快捷键

| 快捷键 | 作用 |
|---|---|
| ⌘/Ctrl + K | 聚焦搜索框 |
| Esc | 清空搜索并返回上一页 / 关闭设置 |
| ⌘/Ctrl + R、F5 | 增量刷新本地数据 |
| ⌘/Ctrl + , | 打开设置 |
| ⌘/Ctrl + F（对话详情内） | 会话内查找，Enter / Shift+Enter 切换命中 |

## 数据路径

### Claude Code

| 数据 | 默认路径 | 用途 |
|---|---|---|
| Prompt 历史 | `~/.claude/history.jsonl` | 输入框历史 |
| 完整会话 | `~/.claude/projects/**/*.jsonl` | 用户/助手消息、工具调用和用量 |
| 会话元数据 | `~/.claude/sessions/*.json` | CLI 版本等补充信息 |

设置中的 `historyFile`、`projectsDir`、`sessionsDir` 可分别覆盖路径；否则从 `claudeDataDir` 推导；仍未配置时使用 `~/.claude`。这些旧字段继续兼容已有 `settings.json`。`importPathMappings` 记录导入会话时确认过的云端目录到本机目录的映射，可在设置里删除。

### OpenAI Codex

| 数据 | 默认路径 | 用途 |
|---|---|---|
| Prompt 历史 | `<CODEX_ROOT>/history.jsonl` | `session_id`、`text`、`ts` |
| 当前会话 | `<CODEX_ROOT>/sessions/YYYY/MM/DD/rollout-*.jsonl` | 完整事件流 |
| 归档会话 | `<CODEX_ROOT>/archived_sessions/*.jsonl` | 已归档事件流 |

`CODEX_ROOT` 的优先级固定为：

1. 设置中的 `codexDataDir`
2. 环境变量 `CODEX_HOME`
3. `~/.codex`

### Antigravity

Antigravity CLI 与 Gemini CLI 共用 `<GEMINI_ROOT>`：设置中的 `geminiDataDir`，留空时为 `~/.gemini`。

| 数据 | 默认路径 | 用途 |
|---|---|---|
| Antigravity CLI 输入历史 | `<GEMINI_ROOT>/antigravity-cli/history.jsonl` | Prompt 与所在工作区 |
| Antigravity CLI 会话 | `<GEMINI_ROOT>/antigravity-cli/brain/<会话 ID>/.system_generated/logs/transcript_full.jsonl`（没有时读 `transcript.jsonl`） | 完整步骤流 |
| 工作区到最近会话 | `<GEMINI_ROOT>/antigravity-cli/cache/last_conversations.json` | 补历史里找不到的工作区 |
| Gemini CLI 会话 | `<GEMINI_ROOT>/tmp/<项目>/chats/session-*.jsonl`、更早的 `session-*.json`、子代理的 `chats/<父会话 ID>/<子会话 ID>.jsonl` | 完整会话 |
| Gemini CLI 项目路径 | `<GEMINI_ROOT>/tmp/<项目>/.project_root`、`<GEMINI_ROOT>/projects.json` | 会话所属项目 |

设置页会分别显示三种产品的配置值、最终解析路径和存在状态。保存设置后重新建立索引，无需重新编译。旧版仅含 Claude 字段的设置文件会自动按默认值补齐 Codex 配置。

设置文件位于系统应用配置目录。macOS 默认是：

```text
~/Library/Application Support/com.xzl.cchistoryviewer/settings.json
```

为兼容旧开发环境，源码根目录的 `settings.json` 仍可作为低优先级回退。示例见 [`settings.example.json`](./settings.example.json)。bundle identifier `com.xzl.cchistoryviewer` 保持不变，因此原有应用配置目录不会丢失。

## Codex 格式兼容

- 从 `session_meta` 读取会话 ID、`cwd`、CLI 版本和客户端来源，从最近的 `turn_context` 读取模型。
- 从 `event_msg.user_message` 提取当前格式的真实用户 Prompt；`history.jsonl` 通过 `session_id` 关联 `session_meta.cwd`。
- 完全旧版 rollout 缺少 `event_msg.user_message` 时，回退到 `response_item` 的 `role=user`；若同一文件中途升级格式，则保留首个 event 之前清理后的旧 Prompt，并从首个 event 起只信任 `event_msg.user_message`。developer、system、`AGENTS.md`、`environment_context`、`codex_internal_context` 等注入内容不会成为 Prompt。
- 从 `response_item` 提取助手消息、函数或自定义工具调用及结果；未知事件会被忽略。
- 旧 rollout 可以只有 `session_meta` 和 `response_item`，缺少模型与 Token 时仍可浏览。
- `history.jsonl` 的 `ts` 同时接受 Unix 秒和毫秒。单行 JSON 损坏不会使整个文件或索引失败。
- 默认 Prompt 与会话统计排除自动生成的 sub-agent Prompt；sub-agent 的真实模型调用仍计入 Token，并归属其自身 `cwd`。

Codex 文件已可能超过 1 GB。解析器逐行读取，不会先把整个 JSONL 加载进内存；会话文件仍按文件并行扫描。

## Antigravity 格式兼容

Antigravity 来源包括 Antigravity CLI（`agy`）和它接替的 Gemini CLI；会话的「来源」显示 `antigravity-cli` 或 `gemini-cli`，Resume 命令随之为 `agy --conversation <ID>` 或 `gemini --resume <ID>`。只读纯文本，不读 Antigravity IDE 与桌面应用，也不读 CLI 的 SQLite 与 protobuf 文件。设计决定见 [Agent Note：第三个来源 Antigravity](.agents/notes/implemented/2026-11-20-antigravity-source.md)。

- **Antigravity CLI**：转录每行是一步。`USER_INPUT` 是用户输入，只取 `<USER_REQUEST>` 块，去掉注入的本地时间与换模型提示；`PLANNER_RESPONSE` 是一次模型调用，含回复、思考、工具调用、模型与 Token；紧随其后的 `GENERIC` 是工具结果，按顺序对应调用。`SYSTEM_MESSAGE` 里的提醒不显示，子代理发回的报告显示为 `subagent` 结果。`transcript_full.jsonl` 的工具参数保留原类型；`transcript.jsonl` 把每个参数值再编码成 JSON 字符串，只在没有前者时读取并还原。
- **工作目录**：转录里没有。先按会话 ID 查 `history.jsonl`；会话第一句输入写入历史时还没有 ID，所以再找 5 分钟内文字相同的历史行；再查 `cache/last_conversations.json`；子代理沿用父会话的。都找不到的会话（部分 `agy -p`）不归入任何文件夹。
- **子代理**：以另一个会话发来的消息开头的 Antigravity CLI 会话，以及 `kind: subagent` 的 Gemini CLI 会话，都是子代理。不单独列为会话和 Prompt，调用并入父会话。
- **Gemini CLI**：按 Gemini CLI 自己的规则回放：首行元数据；带 `id` 的行新增消息，同 ID 替换；`$set` 更新元数据，带 `messages` 时整体替换；`$patch` 修改内容、补工具结果、删除或重排；`$rewindTo` 删掉该消息及之后的消息。更早的 `session-*.json` 是一整个 JSON。注入的 `<session_context>`、`<hook_context>` 不算 Prompt。项目路径取 `.project_root` 或 `projects.json`；目录仍以路径 SHA-256 命名的旧会话，与所有已知工作目录比对哈希。

## 统一模型与筛选口径

公共领域模型使用：

- `agent`: `claude | codex | antigravity`
- `origin`: `history | conversation | both`

`origin` 只描述 Prompt 来自输入历史、会话文件或两者，不再与产品来源混用。Prompt、项目、会话、消息、用量和缓存记录均保留 `agent`。稳定 ID、会话映射、路由、去重键和缓存键都包含 `agent`，因此两个产品即使出现相同 `session_id` 也不会冲突；跨产品的相同文本不会互相去重。

“全部”筛选满足以下恒等关系：

- Prompt 数、会话数和每个归一化 Token 分项等于三个产品之和。
- 项目数等于各产品 `cwd` 路径的并集大小，不是项目数简单相加。
- 相同 `cwd` 的项目在导航中合并，但其 Prompt 和会话仍保留 `agent`。

## Token 统计

统一用量字段如下：

| 字段 | 含义 |
|---|---|
| `uncachedInput` | 未命中缓存的输入 Token |
| `cacheRead` | 从缓存读取的输入 Token |
| `cacheCreation` | 创建缓存使用的输入 Token |
| `cacheCreation1h` | `cacheCreation` 中按 1 小时档计费的部分（仅 Claude），其余为 5 分钟档 |
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
cacheCreation1h = cache_creation.ephemeral_1h_input_tokens（缺省 0，不超过 cacheCreation）
output = output_tokens
```

Codex 映射只使用每个 `token_count.info.last_token_usage`，不累加累计字段 `total_token_usage`：

```text
uncachedInput = input_tokens - cached_input_tokens
cacheRead = cached_input_tokens
cacheCreation = 0
cacheCreation1h = 0
output = output_tokens
reasoningOutput = reasoning_output_tokens
```

Antigravity CLI 的 `input_tokens` 不含缓存读取，`output_tokens` 已含思考但纯文本里没有拆分：

```text
uncachedInput = input_tokens
cacheRead = cache_read_tokens
output = output_tokens
reasoningOutput = 0
```

Gemini CLI 的 `tokens.input` 已含缓存命中部分，思考与工具调用提示 Token 另计（`total = input + output + thoughts + tool`）：

```text
uncachedInput = input - cached + tool
cacheRead = cached
output = output + thoughts
reasoningOutput = thoughts
```

Codex 的 `input_tokens` 已包含 `cached_input_tokens`，所以不能再把两者作为两个完整输入量相加。fork、resume 或派生会话可能复制历史 Token 事件；索引使用不依赖文件路径和新会话 ID 的稳定事件指纹跨文件去重。

缓存命中率为：

```text
cacheRead / (uncachedInput + cacheRead)
```

分母为 0 时显示“—”。概览、按天、按模型和按项目的总 Token 均使用同一公式。

每个会话的 Token 与成本按同一去重规则归属：按会话开始时间顺序，一条调用首次出现在哪个会话就计入哪个会话，fork/resume 复制的历史调用只计入原会话；子代理的调用并入父会话。因此各会话之和等于全局总量。

对话详情页的「Token 用量明细」面板与这套归属完全同源：索引记住每条唯一调用属于哪个会话，详情按此列出本会话的全部调用（含子代理文件里的调用），按调用与按模型的 Token 之和都等于页头的会话总量。每条助手消息上的用量标签来自该记录自身：Claude 一次调用按内容块拆成多条记录，只有首条带用量；Codex 的 `token_count` 事件归到它前面最近一条模型输出的消息（工具结果不算）。由 resume/fork 复制进来、已计入原会话的调用会灰显标注，不计入本会话的任何数字。缓存节省 = 全部输入按非缓存价计的成本 − 估算成本，缓存写多于读时为负。

面板也显示每次调用的推理档位，取记录里的原值（`low` / `medium` / `high` / `xhigh` / `max`，Codex 还有 `minimal`）：Claude Code 读助手记录顶层的 `perTurnEffort`，缺失时用 `effort`；Codex 读每轮 `turn_context.effort`，同一轮的调用共用。旧版本的记录没有档位，显示「未记录」，不推测默认值。Antigravity 的记录也没有档位字段，档位写在模型名里（如 `gemini-3.8-flash-low`），模型名照原样显示。档位出现在消息用量标签与调用图的悬停、输出卡片（多档位时列出各档位调用次数，含子代理），按模型表在同一模型下按档位分行。档位不参与成本估算。设计决定见 [Agent Note：用量明细显示推理档位](.agents/notes/implemented/2026-11-19-usage-reasoning-effort.md)。

## 成本说明

成本按产品和可靠匹配的具体模型分别估算。Codex 成本显示为 **API 等价估算**，仅表示按对应 OpenAI API 标准 Token 单价换算的参考值，不代表 ChatGPT 或 Codex 订阅的实际账单、额度或积分消耗。

无法可靠匹配价格的模型显示“—”。合并统计只累加已知价格的估算成本，并显示未知价格 Token 的覆盖提示；不会把未知价格当作零成本。Antigravity 的调用按调用当天适用的 Gemini API 标准价估算（2026-11-26 对照 [Gemini API 定价](https://aistudio.google.com/docs/pricing) 核对），不代表 Google AI 订阅实际支出；定价页已不再列出的下线 Gemini 模型，以及经 Antigravity 使用的其他厂商模型，成本显示“—”。见 [Agent Note](.agents/notes/proposed/2026-11-26-gemini-pricing.md)。内置价格于 2026-07-17 对照 [Anthropic 定价](https://platform.claude.com/docs/en/about-claude/pricing) 与 OpenAI 官方模型页（例如 [GPT-5.5](https://developers.openai.com/api/docs/models/gpt-5.5)、[GPT-5.4](https://developers.openai.com/api/docs/models/gpt-5.4)、[GPT-5.4 mini](https://developers.openai.com/api/docs/models/gpt-5.4-mini)、[GPT-5.1 Codex Max](https://developers.openai.com/api/docs/models/gpt-5.1-codex-max)、[o4-mini](https://developers.openai.com/api/docs/models/o4-mini) 与 [GPT-4.1](https://developers.openai.com/api/docs/models/gpt-4.1)）复核；模型价格变化后应重新核验。

Claude 缓存写按 5 分钟档（输入价 ×1.25）与 1 小时档（输入价 ×2）分别计价；旧记录没有分档字段时全部按 5 分钟档。

## 增量缓存

索引缓存 schema 为 **v7**（v7 为每条调用增加推理档位字段，升级后首次启动会全量重建一次索引）。每条 history/会话文件缓存记录包含 `agent`，缓存键包含产品和文件身份；文件 `mtime` 与长度指纹未变化时复用解析结果，仅重新解析新增或变化的文件。删除文件、路径设置变化或 cache schema 版本变化会使对应缓存失效。Claude 与 Codex 扫描均可并行进行。

缓存写在本应用的系统数据目录，不写入 `~/.claude` 或 `~/.codex`。缓存是本地派生数据，可能含解析后的 Prompt、消息摘要和用量；它与原始历史一样应按敏感数据保护。

索引在后台线程构建，构建期间界面显示扫描与解析进度，其他查询等待同一次构建完成。顶栏刷新按钮与 ⌘/Ctrl+R 只重解析指纹变化的文件；设置中的“全量重建”忽略缓存重新解析全部文件。重建期间旧索引继续可用。

## 全文搜索

搜索页的“会话内容”模式不建立任何索引：每次搜索并行流式扫描当前筛选范围内的会话文件，先用大小写不敏感的字节子串预筛原始行，只解析命中的行。覆盖用户消息、助手回复、思考摘要与工具调用参数；工具结果正文（文件内容、命令输出）不参与匹配，developer/system 与 `AGENTS.md` 等注入内容也不会命中。多个关键词需同时出现在同一条消息中。每个会话最多返回 20 条、总计最多 300 条，超出时提示缩小范围。

## 导入 Claude Code 会话

在云端 Claude Code 里让它把会话记录打包成 zip（保持 `~/.claude` 的原始层级），下载到本机后点击顶栏的「导入」：

```text
projects/<项目目录名>/<会话ID>.jsonl
projects/<项目目录名>/<会话ID>/          ← 同名旁挂目录：tool-results/*.txt、可能有 subagents/
```

- **写入位置**：解压到当前配置的 Claude projects 目录（默认 `~/.claude/projects`），去掉 zip 里的 `projects/` 前缀。导入的会话与本机会话一样受 Claude Code 默认 30 天的记录清理影响。
- **安全**：只接受 `projects/<目录名>/` 之下的条目；绝对路径、含 `..` 的路径和符号链接会让整个导入包被拒绝，什么都不写。`__MACOSX/`、`.DS_Store` 等 `projects/` 之外或无法归入会话的条目跳过并在汇总里列出。
- **项目映射**：云端会话的工作目录是 `/home/user/<仓库名>`。导入弹窗按仓库名预填本机已索引的项目，也可用文件夹选择器指定，或保持原路径。指定本机目录后，只改写每行顶层的 `cwd` 字段，其余内容原样保留，会话写入本机路径对应的项目目录，viewer 归到该项目，本机 `claude --resume <会话ID>` 也能找到它。确认过的映射记在设置文件里，下次导入同一仓库自动预填。
- **计划规则**：jsonl 与旁挂目录作为一个会话整体判定。本机没有：新增；本机 jsonl 是导入 jsonl 的前缀（同一会话更新后的快照）：覆盖更新；导入 jsonl 是本机 jsonl 的前缀：跳过；完全相同：跳过，只补导入包里多出的旁挂文件；其余情况，以及 jsonl 相同但旁挂目录里有内容不同的同名文件：冲突。`ccr-tip.json` 这类运行时状态文件不参与比较。
- **冲突处理**：弹窗逐个会话显示会话 ID、所属项目和两边的行数与最后一条记录时间，选择「保留本机」或「用导入的覆盖」，选择对整个会话生效；取消则什么都不写。
- **写入与汇总**：先解析出计划，确认后一次性写入，写入前重新校验；只覆盖同名文件，绝不删除本机文件。写完走增量重建索引，并显示新增、更新、跳过、冲突中保留本机、冲突中覆盖各多少个会话。

## 外部会话

别人用本应用导出的单会话 Markdown（对话详情页的「导出 Markdown」）可以在侧栏「外部会话」里点「导入 Markdown」临时阅读，可一次多选。它们不会成为本机会话。设计决定见 [Agent Note：外部会话](.agents/notes/implemented/2026-10-10-external-markdown-sessions.md)。

- **支持的文件**：单会话导出（标题为「# 对话导出」或「# Conversation Export」），中英文标签都识别，Claude Code、Codex 与 Antigravity 都支持。多文件批量导出里每个会话的文件与单会话导出格式相同，也可以导入；合并成一份的批量导出、index.md、Prompt 导出和搜索结果导出会被拒绝并说明原因。
- **详细版与简洁版**：按产品 + 会话 ID 合并为一条。含思考、工具调用、工具结果或附件的文件算详细版，其余算简洁版；同一版本再次导入时替换旧的。「完整」显示详细版；「只看对话」显示简洁版，没有简洁版时由详细版去掉思考、工具调用、工具结果与附件得到，规则与导出时不勾选「包含工具调用与思考过程」相同。只导入了简洁版时只能看对话。
- **还原程度**：消息按文件里的分钟级时间原样显示；同一条消息里的多个文本块合并为一段；工具调用参数按 JSON 还原，按语义展示；工具结果标出所属工具（旧版导出按调用顺序推断）；工具结果与附件保留导出时的全文，不套用本机会话的展示截断。导出里没有逐次调用的用量，所以不显示 Token 用量明细面板，只显示文件头里的 Token 总量与估算成本。
- **临时与保留**：导入的会话默认只在内存里，关闭应用即消失。点「保留」把原始 Markdown 复制到应用数据目录的 `external_sessions/<产品>-<会话ID>/`，下次启动自动载入；已保留的会话再导入新版本时同步写入。「删除」移除该条目，已保留的会同时删除这个目录。
- **隔离**：外部会话不写入 `~/.claude` 或 `~/.codex`，不进入项目列表、概览统计、Prompt 搜索、全文搜索和 Token 汇总，也不提供再次导出与 Resume 命令。外部会话里 Markdown 引用的远程图片不会自动加载，只显示为「远程图片：…」链接，点击才交给系统浏览器打开，避免对方借图片得知你打开了文件。

## 超大工具输出与附件

Claude Code 会把超大的工具输出写到 `projects/<目录名>/<会话ID>/tool-results/*.txt`，并在 tool_result 里留下 `<persisted-output>` 块和生成时的绝对路径。详情页按会话自己的目录和 `projects/` 之后的部分在当前 projects 目录下重新定位，`/` 与 `\` 分隔符都支持；找到就出现「查看完整输出」按钮，找不到保留原预览。导出包含工具调用时会内联完整输出并标注来源文件。只在显示层处理，不改写 jsonl。

@ 引用或上传的文件内容记录在 `attachment` 行里，详情页以「附件：文件名」折叠块紧跟在对应用户消息之后，导出包含工具调用时一并写入。云端会话的 thinking 块只有 signature、正文为空，这类块不再显示。

## 应用内更新

应用读取本仓库最新 Release 里的 `latest.json`，版本号比当前高才提示。设计决定见 [Agent Note：应用内检查更新与一键更新](.agents/notes/implemented/2026-10-11-in-app-updates.md)。

- **检查**：应用开着时每个整点（本机时间）自动检查一次；启动后和窗口重新获得焦点时，本小时还没查过也查一次。检查失败也算本小时查过，下个整点再试。电脑睡眠错过整点时，切回窗口或每 5 分钟的兜底会补查。上次发现的新版本还没装时，每次启动都会再查，让提示一直在。检查失败不弹提示，原因显示在设置的「更新」里。设置里可以关闭自动检查，也可以随时手动检查。检查时机见 [Agent Note](.agents/notes/implemented/2026-11-20-update-check-timing.md)。
- **安装**：点顶栏的「新版本」查看更新内容，再点「立即更新」。跳过了好几个版本时，从新到旧列出最近 3 个版本的更新内容，更多时可在浏览器查看全部版本；更新内容只显示应用当前界面语言那部分。见 [Agent Note](.agents/notes/proposed/2026-11-25-missed-release-notes.md)。更新包下载后先用应用内置的公钥核对签名，对不上就拒绝安装。macOS 原地替换应用后重新打开；Windows 由安装程序装完再打开（只显示进度条），msi 版会请求一次管理员确认。设置、索引缓存与保留的外部会话都在应用数据目录，更新不受影响。
- **网络**：请求跟随系统代理。连不上 GitHub 时，可在设置里点「打开下载页」手动下载。
- **第一次**：v0.12.0 及更早的版本没有这项功能，需要手动下载安装一次 v0.13.0 或更新的版本。

## 隐私边界

- 不读取 Codex `auth.json`。
- 不把 Codex 私有 SQLite 表作为主要数据源。
- 不读取 Antigravity 的凭据、SQLite 与 protobuf 文件，不写入 `~/.gemini`。
- 不上传、不遥测历史内容。联网只用于检查和下载更新：只请求本仓库 GitHub Release 上的版本清单和更新包，请求里不带任何历史内容；自动检查可在设置中关闭。
- 不修改或删除 `~/.codex` 及自定义 Codex 目录中的任何内容；对 `~/.claude` 唯一的写入是用户主动确认的会话导入，且只写 `projects/<目录名>/` 之下的会话文件，只覆盖同名文件，绝不删除。
- 应用只写自身设置、索引缓存、窗口位置状态、用户主动导出的 Markdown 文件、用户主动导入的会话文件、用户选择保留的外部会话 Markdown（应用数据目录内），以及用户确认安装更新时替换应用自身；全文搜索按需读取会话文件，不写任何索引。
- 对话里的链接只在用户点击时交给系统浏览器打开。本机会话 Markdown 里的远程图片会由界面直接加载；外部会话里的远程图片只显示为链接。

## 开发与验证

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

## 许可证

[MIT](./LICENSE)
