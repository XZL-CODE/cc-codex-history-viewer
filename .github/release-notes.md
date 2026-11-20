## v0.15.0 更新内容 / What's new

- **新增第三个来源 Antigravity**：读取 Antigravity CLI（`agy`）与它接替的 Gemini CLI 在 `~/.gemini` 下的本机会话，和 Claude Code、Codex 一样按文件夹浏览、搜索、导出和统计 Token。概览与侧栏可以按 Antigravity 筛选；会话显示来自哪个 CLI，Resume 命令随之不同；子代理的调用并入父会话。Antigravity 的模型暂未定价，只统计 Token。数据目录可在设置里修改，默认 `~/.gemini`。Antigravity IDE 与桌面应用的对话不在范围内。
- **更及时地发现新版本**：应用开着时也会自动检查更新，窗口切回来时和运行期间定时检查，每小时最多一次；检查失败一小时后自动再试。之前只在启动时检查，每天最多一次。
- 装着 v0.13.0 及之后版本的应用可以在应用内一键更新；v0.12.0 及更早的版本仍需手动下载安装。

- **New source: Antigravity**: reads the local sessions of Antigravity CLI (`agy`) and the Gemini CLI it replaced under `~/.gemini`, browsable, searchable, exportable and counted in token usage like Claude Code and Codex. The overview and sidebar can filter by Antigravity; a session shows which CLI wrote it and gets the matching resume command; sub-agent calls roll up into the parent session. Antigravity models are not priced yet, so only tokens are counted. The data directory defaults to `~/.gemini` and can be changed in Settings. Conversations from the Antigravity IDE and desktop app are not included.
- **New versions show up sooner**: the app now also checks for updates while it is open, when you switch back to its window and periodically, at most once an hour; a failed check is retried an hour later. Previously it checked only at startup, at most once a day.
- Apps on v0.13.0 or later can update in-app; v0.12.0 and earlier still need a manual install.
