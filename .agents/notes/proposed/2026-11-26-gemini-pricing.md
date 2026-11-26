# Agent Note: Antigravity 按 Gemini API 价格估算成本

Status: proposed

## Problem

Antigravity 来源上线时 Gemini 模型不定价，调用只统计 Token，成本显示「—」。Antigravity CLI 默认用 Gemini 4 Neon，
单个会话就有几千万 Token，概览和会话详情都看不到成本。CLI 可选的模型除了 Gemini，还有 Claude Opus 5.5、Claude
Sonnet 5.5 与 GPT-OSS 120B；部分 Gemini 模型当前是限时优惠价，到期后翻倍。

## Proposal

- 价格继续写在 `pricing.rs` 里，新增 Gemini 表，按 Gemini API Standard 付费价。理由：用户在硬编码、远程价格表、
  第三方价格库与用户自填之间选择硬编码；与 Claude、Codex 一致，不新增联网，价格可审查、可测试；改价随发版，
  应用内更新每个整点检查。
- 覆盖官方定价页上的全部 Gemini 文本模型。Antigravity CLI 的模型名先去掉推理档位后缀（`gemini-4-neon-xhigh` →
  `gemini-4-neon`），再按完整 API 模型 ID 精确匹配。定价页不再列出价格的已下线模型，以及经 Antigravity 使用的
  其他厂商模型，保持未知定价。理由：用户要求覆盖全部 Gemini 模型；价格必须有官方来源，查不到的不猜。
- 每次调用按发生当天（UTC）适用的价格计：价格行可带生效日，来自定价页写明的调价和下线模型改按新模型计费的日期。
  定价页没写起始日期的价格视为自发布起有效。理由：用户选择按调用时的价格；按现价重算会在每次调价后改动历史成本。
- Pro 模型提示超过 20 万 Token 时按高一档计；隐式缓存命中按缓存读价；显式缓存的按小时存储费不计。理由：与定价页
  一致；存储时长从会话记录里看不出来。
- 金额与 Codex 一样是 API 等价估算，不代表 Google AI 订阅的实际支出，界面说明写明这一点。理由：Antigravity CLI
  主要通过 Google AI 订阅使用。

## Alternatives considered

- 应用定期从本仓库拉取价格表：不发版也能改价，但多一种联网请求，隐私说明要改，还要处理离线与可信度。
- 第三方价格库（如 LiteLLM、OpenRouter）：新模型自动覆盖，但要访问第三方服务器，数据质量参差。
- 设置里让用户自填价格：覆盖面最广，但维护成本在用户身上，可以以后作为补充。
- 覆盖定价页上的全部 Gemini 模型，并按调用当天的价格计（含优惠期与下线模型改按新模型计费的日期）：曾实现过，
  用户改为只覆盖 CLI 当前模型、固定按原价，估算不随促销起落，也不用维护价格的时间线。

## Consequences

- 新模型或调价要发版更新价格表；发版前新模型显示未知定价。
- 限时优惠期内的估算高于当时的实际 API 价格（按原价计）。
- 显式缓存存储费与 Antigravity 不记录的缓存写不计，Claude 调用的成本因此偏低。
- Claude 表补上 Opus 5.5 与 Sonnet 5.5 后，Claude Code 来源也受益：Opus 5.5 有了成本，Sonnet 5.5 的缓存读不再
  沿用 Sonnet 5 的 0.20，改为官方的 0.10。

## 与已有决定的关系

[第三个来源 Antigravity](../implemented/2026-11-20-antigravity-source.md) 的 Proposal 写明「Gemini 模型暂不定价」。
用户裁定改为本 Note 的方向；该条已标待裁定。

## Verification

- `pricing.rs` 单元测试：Antigravity 的 Gemini 模型去掉推理档位后缀后按原价计（Carbon、Argon、3.8 Flash 不用优惠价）；
  Antigravity 的 `claude-` 模型按 Claude 表计；Claude Opus 5.5 与 Sonnet 5.5 有自己的价格，Sonnet 5.5 缓存读为 0.10；
  GPT-OSS 120B、`orchestrator`、表外 Gemini 模型不计价，Gemini 模型名在其他来源下不计价。
- `tests/antigravity_index.rs`：合成样本会话的 Gemini 3.8 Flash 与 Claude Sonnet 4.6 调用都计价，金额与手算一致，
  未知定价 Token 为 0。
- 价格于 2026-11-26 对照官方页面核对：Gemini 价格逐字取自 Gemini API 定价页；Claude 5.5 取自 Anthropic 定价页；
  Google Cloud 现行定价页没有 GPT-OSS 120B。尚未在真实 `~/.gemini` 数据上核对金额，Orchestrator 在记录里的模型名
  也还没见过。

## Related notes

- [第三个来源 Antigravity](../implemented/2026-11-20-antigravity-source.md)
