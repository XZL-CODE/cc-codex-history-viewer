# Agent Note: 第三个来源 Antigravity

Status: proposed

## Problem

应用只读 Claude Code 与 Codex。Google 已在 2026 年 6 月 18 日停止向免费与 AI Pro/Ultra 用户提供 Gemini CLI，由
Antigravity CLI（`agy`）接替；企业授权与付费 API key 用户仍可使用 Gemini CLI。两者的会话都存在 `~/.gemini` 下，
格式互不相同，也没有官方文档。需要决定第三个来源叫什么、覆盖哪些产品、读哪些文件，以及 Token、成本与推理档位怎么处理。

## Proposal

- 来源名为 Antigravity，内部 ID 为 `antigravity`，概览筛选增加这一项。理由：与 Claude Code、Codex 同为产品名；Google
  把 Antigravity CLI 定位为 Gemini CLI 的接替者；Gemini 是模型名，Antigravity 平台也能选用其他厂商的模型。内部 ID 是
  稳定 ID、路由与缓存键的一部分，以后不改。
- 覆盖 Antigravity CLI 与旧版 Gemini CLI 的本机会话，不做 Antigravity IDE 与桌面应用。Gemini CLI 同时读当前的追加式
  `session-*.jsonl` 与更早的单文件 `session-*.json`。理由：IDE 的对话正文 `conversations/*.pb` 在样本中无法解码；
  更早使用 Gemini CLI 的人留下的多是 `.json`。
- 只读纯文本：Antigravity CLI 的 `brain/<id>/.system_generated/logs/transcript_full.jsonl` 与 `history.jsonl`，
  Gemini CLI 的 `tmp/<项目>/chats/` 会话文件、`.project_root` 与 `projects.json`。不读 SQLite 与 protobuf。理由：
  正文、模型与 Token 在纯文本里都有；二进制格式没有文档、不开源，随时可能变。若工具结果只存在数据库里，另行讨论。
- 数据目录默认 `~/.gemini`，设置项为 `geminiDataDir`。理由：两个 CLI 共用这个目录。
- Gemini 模型暂不定价：Token 照常统计，成本按未知定价模型处理。理由：用户确认这次不加价格表。
- 推理档位只取记录里的原值，不从模型名后缀（如 `gemini-3.8-flash-high`）拆出档位，模型名原样显示。理由：沿用
  [推理档位 Note](./2026-11-19-usage-reasoning-effort.md) 的原值规则。
- Gemini CLI 的 Token 归一：缓存读 = `cached`；非缓存输入 = `input − cached + tool`；输出 = `output + thoughts`，其中
  `thoughts` 记为推理输出。理由：样本中 `total = input + output + thoughts + tool`；按 Gemini API 的用量语义，缓存
  命中的部分包含在 `input` 里。Antigravity CLI 的归一等第二份样本确认 `input_tokens` 是否包含 `cache_read_tokens`。

## 读取规则

依据两份本机样本报告（Antigravity CLI 1.5.13、Gemini CLI 0.45）与 Gemini CLI 公开源码中的回放规则，细节见
[README 的 Antigravity 格式兼容](../../../README.zh-CN.md#antigravity-格式兼容)。

- 子代理（以另一个会话的消息开头的 Antigravity CLI 会话、`kind: subagent` 的 Gemini CLI 会话）不单独列为会话与
  Prompt，调用并入父会话，与 Claude 子代理一致。
- Antigravity CLI 转录没有工作目录，按 `history.jsonl` 的会话 ID、5 分钟内同文本的历史行、
  `cache/last_conversations.json` 依次补全，子代理沿用父会话；都找不到时不归入文件夹（部分 `agy -p` 会话）。
- 工具结果（`GENERIC`）按顺序对应前面的工具调用；`SYSTEM_MESSAGE` 里的提醒不显示，子代理发回的报告显示为结果。
- 注入的上下文（`<USER_REQUEST>` 之外的本地时间与换模型提示、Gemini CLI 的 `<session_context>`）不算 Prompt。

## Risks

- 工具结果与调用只能按顺序配对：转录不带调用 ID（ID 只在 SQLite 里）。并行或异步调用乱序、缺结果、重试时可能配错。
- 样本里两份转录相同，压缩上下文后的转录形状、`truncated_fields` 的含义都还没见过；转录被重写变短时按整文件重读。
- 推理档位只体现在模型名后缀；`agy --effort max` 在不支持 max 的模型上被降为 high，记录里看不出请求过 max。
- Gemini CLI 的 `cached ⊂ input` 来自 API 语义与 `total` 的等式，本机样本缓存都为 0，未用真实缓存命中核对。
- Antigravity CLI 闭源，格式随版本变化只能事后跟进。

## Verification

`tests/antigravity_index.rs` 用合成的 `~/.gemini` 目录树（`tests/fixtures/antigravity/`，内容为虚构）锁定：转录的
消息结构、工具结果配对、去掉注入内容、子代理报告、两种转录的参数还原；Antigravity CLI 与 Gemini CLI 的 Token 归一；
Gemini CLI 的 `$set`、`$patch`、`$rewindTo` 与旧 `.json`；工作目录按历史文字、`last_conversations.json`、`.project_root`
与哈希补全；子代理调用并入父会话且详情的调用之和等于会话总量；全文搜索命中的消息 ID 与详情一致。
`external.rs` 的导出往返测试覆盖 Antigravity。界面用模拟后端在浏览器中核对了筛选、标识色、设置项与对话详情。
尚未在装有真实数据的应用里验证。
