## v0.8.0 更新内容 / What's new

- **批量导出会话**：文件夹页「会话」标签页新增「选择会话」模式，勾选多个 Claude Code / Codex 会话一次导出。可「合成一份」（带目录表，每个会话一节），或「每个会话一份」（文件夹内每会话一个 .md，另附 index.md）；可选是否包含工具调用与思考过程。（回应 issue #2）
- **导出内容不再截断**：单会话与批量导出改走不截断的解析路径，超过界面展示上限（24,000 字符）的工具结果和长回复完整保留；若导出数据带截断标记，Markdown 中会明确标出。
- 批量导出显示进度，完成后可直接在 Finder / 资源管理器中定位导出结果。

- **Batch session export**: a new Select mode on a folder's Sessions tab exports several Claude Code / Codex sessions at once, either as one merged Markdown file with a table of contents or as one file per session plus an index.md; tool calls and thinking are optional. (Addresses issue #2)
- **Exports are no longer clipped**: single and batch exports parse without the 24,000-character display limit, so long tool results and replies are kept whole; any clipped block that still reaches an export is marked explicitly.
- Batch exports show progress, and the result can be revealed in Finder / Explorer.
