## v0.14.0 更新内容 / What's new

- **Token 用量明细显示推理档位**：对话详情的用量明细能看到每次调用用的推理档位（如 `high`、`xhigh`），显示 Claude Code 与 Codex 记录里的原值。消息用量标签和调用图的悬停提示里有档位；输出卡片汇总会话用了哪些档位、各多少次；按模型表里同一模型按档位分行，可以对比不同档位的输出量和成本。旧版本 CLI 的记录没有档位，显示「未记录」。
- 升级后第一次启动会全量重建一次索引，之后照常增量刷新。
- 装着 v0.13.0 及之后版本的应用可以在应用内一键更新；v0.12.0 及更早的版本仍需手动下载安装。

- **Reasoning effort in token usage details**: the conversation's usage details now show the reasoning effort each call ran at (such as `high` or `xhigh`), as Claude Code and Codex record it. It appears in the message usage chip and call chart tooltips; the output tile sums up which efforts the session used and how often; the per-model table splits each model by effort so you can compare output and cost across efforts. Records from older CLI versions carry no effort and show "not recorded".
- The first launch after upgrading rebuilds the index once; refreshes are incremental afterwards.
- Apps on v0.13.0 or later can update in-app; v0.12.0 and earlier still need a manual install.
