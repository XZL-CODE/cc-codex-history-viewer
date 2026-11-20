## v0.14.0 更新内容 / What's new

- **新增第三个来源 Antigravity**：读取 Antigravity CLI（`agy`）与它接替的 Gemini CLI 在 `~/.gemini` 下的本机会话，和 Claude Code、Codex 一样按文件夹浏览、搜索、导出和统计 Token。概览与侧栏可以按 Antigravity 筛选；会话显示来自哪个 CLI，Resume 命令随之不同；子代理的调用并入父会话。Antigravity 的模型暂未定价，只统计 Token。数据目录可在设置里修改，默认 `~/.gemini`。Antigravity IDE 与桌面应用的对话不在范围内。
- **Token 用量明细显示推理档位**：对话详情的用量明细能看到每次调用用的推理档位（如 `high`、`xhigh`），显示 Claude Code 与 Codex 记录里的原值。消息用量标签和调用图的悬停提示里有档位；输出卡片汇总会话用了哪些档位、各多少次；按模型表里同一模型按档位分行，可以对比不同档位的输出量和成本。旧版本 CLI 的记录没有档位，显示「未记录」。
- **更及时地发现新版本**：应用开着时也会自动检查更新，窗口切回来时和运行期间定时检查，每小时最多一次；检查失败一小时后自动再试。之前只在启动时检查，每天最多一次。
- 升级后第一次启动会全量重建一次索引，之后照常增量刷新。
- 装着 v0.13.0 及之后版本的应用可以在应用内一键更新；v0.12.0 及更早的版本仍需手动下载安装。

- **New source: Antigravity**: reads the local sessions of Antigravity CLI (`agy`) and the Gemini CLI it replaced under `~/.gemini`, browsable, searchable, exportable and counted in token usage like Claude Code and Codex. The overview and sidebar can filter by Antigravity; a session shows which CLI wrote it and gets the matching resume command; sub-agent calls roll up into the parent session. Antigravity models are not priced yet, so only tokens are counted. The data directory defaults to `~/.gemini` and can be changed in Settings. Conversations from the Antigravity IDE and desktop app are not included.
- **Reasoning effort in token usage details**: the conversation's usage details now show the reasoning effort each call ran at (such as `high` or `xhigh`), as Claude Code and Codex record it. It appears in the message usage chip and call chart tooltips; the output tile sums up which efforts the session used and how often; the per-model table splits each model by effort so you can compare output and cost across efforts. Records from older CLI versions carry no effort and show "not recorded".
- **New versions show up sooner**: the app now also checks for updates while it is open, when you switch back to its window and periodically, at most once an hour; a failed check is retried an hour later. Previously it checked only at startup, at most once a day.
- The first launch after upgrading rebuilds the index once; refreshes are incremental afterwards.
- Apps on v0.13.0 or later can update in-app; v0.12.0 and earlier still need a manual install.
