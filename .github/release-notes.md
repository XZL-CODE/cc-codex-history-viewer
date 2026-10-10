## v0.11.0 更新内容 / What's new

- **外部会话**：侧栏新增「外部会话」，导入别人用本应用导出的单会话 Markdown，用同一套对话详情阅读（Markdown 渲染、工具调用语义展示、会话内查找、大纲）。可一次多选；合并成一份的批量导出、index.md、Prompt 与搜索结果导出会被拒绝并说明原因。
- **详细版与简洁版合并**：同一会话的详细版（含思考与工具调用）和简洁版合并为一条，详情页在「完整 / 只看对话」之间切换；没有简洁版时由详细版过滤得到。
- **默认临时，可保留**：导入的会话默认只在本次运行中保留，关闭应用即消失；点「保留」才把原始 Markdown 复制到应用数据目录，重启后仍在，也可随时删除。不写入 `~/.claude` 或 `~/.codex`，不计入统计与搜索；外部会话里的远程图片只显示为链接，点击才打开。

- **External sessions**: a new "External sessions" page in the sidebar imports a single-conversation Markdown file that someone else exported with this app and reads it in the same conversation view (Markdown rendering, semantic tool-call blocks, in-conversation find, outline). Several files can be picked at once; merged batch exports, index.md, prompt exports and search-result exports are rejected with a reason.
- **Detailed and brief merged**: the detailed export (with thinking and tool calls) and the brief export of one session become one entry with a "Full / Dialogue only" switch; without a brief file the dialogue is filtered from the detailed one.
- **Temporary by default, keep if you like**: imports last only for the current run; "Keep" copies the original Markdown into the app's data directory so it survives restarts, and it can be deleted at any time. Nothing is written to `~/.claude` or `~/.codex`, external sessions stay out of stats and search, and remote images in them show as links that open only when clicked.
