# Agent Note: Antigravity 按 Gemini API 价格估算成本

Status: proposed

## Problem

Antigravity 来源上线时 Gemini 模型不定价，调用只统计 Token，成本显示「—」。Antigravity CLI 默认用 Gemini 4 Neon，
单个会话就有几千万 Token，概览和会话详情都看不到成本。Gemini 的价格变动频繁：有限时优惠价、到期翻倍，下线模型的
请求还会改按新模型计费。

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
- 全部按现价计：实现最简单，但 Gemini 4 Carbon / Argon 12 月翻倍后，11 月的调用成本也会跟着翻倍。

## Consequences

- 新模型或调价要发版更新价格表；发版前新模型显示未知定价。
- 时间维度只覆盖现行定价页写明的调价与改道日期。定价页不保留旧价格，网页存档在核对时无法访问，此前若有过调价，
  旧调用仍按现行价计。
- 显式缓存存储费不计，用显式缓存时成本偏低；Antigravity CLI 的记录看不出缓存是隐式还是显式。

## 与已有决定的关系

[第三个来源 Antigravity](../implemented/2026-11-20-antigravity-source.md) 的 Proposal 写明「Gemini 模型暂不定价」。
用户裁定改为本 Note 的方向；该条的标注待用户确认后修改。

## Verification

- `pricing.rs` 单元测试：去掉推理档位后缀后匹配；Carbon / Argon 在 2026-11-30 与 2026-12-01、3.8 Flash 在
  2026-12-31 与 2027-01-01 的价格；2.5 Pro 在 2026-12-01 起改按 3.8 Flash 计；3.1 Pro 的 20 万 Token 分档且 Flash
  不分档；3 Pro Preview 在 2026-03-09 前未知、之后按 3.1 Pro 计；已下线模型与 Antigravity 下的 Claude 模型不计价；
  Claude 与 Codex 的价格不随调用日期变化；价格行的生效日合法且按时间排列。
- `tests/antigravity_index.rs`：合成样本会话的 Gemini 调用按 3.8 Flash 的 2026-11 价格计，Claude 调用计入未知定价。
- 价格于 2026-11-26 对照官方定价页逐字核对（2.5 系列另做了一次逐字复核）。尚未在真实 `~/.gemini` 数据上核对金额。

## Related notes

- [第三个来源 Antigravity](../implemented/2026-11-20-antigravity-source.md)
