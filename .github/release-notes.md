## v0.12.0 更新内容 / What's new

- **对话 Markdown 渲染修正**：中文标点旁的粗体、斜体与删除线（如「**不会当作零。**其他」）正确显示；段内单个换行按换行显示；思考块折叠时不再显示「**标题**」标记；裸网址不再把后面的中文标点吞进链接；表格里的 `<br>` 换行、HTML 注释隐藏。
- **子代理报告与计划**：Task / Agent 的指令与返回报告、ExitPlanMode 的计划跟随「Markdown / 原文」开关按 Markdown 渲染；命令输出等其他工具结果仍是原文。
- **Codex 会话**：回复末尾的建议追问显示为「↪」小标签，悬停看完整提问；代码模式的工具输出直接显示文本，不再是带 `\n` 转义的 JSON。
- **标题与大纲**：会话列表标题、大纲轮次预览、外部会话标题改为去掉标记的纯文本，跳过 Codex 注入的「Files mentioned by the user」等上下文，直接显示真正的提问。
- **本地文件链接**：指向本机路径的链接不再是点了没反应的空链接，改为虚线下划线，悬停显示路径。
- **导出**：工具结果标题带上工具名（「↩ 工具结果 · Bash」），外部会话导入时据此标出结果所属工具。

- **Markdown rendering fixes**: bold, italic and strikethrough next to CJK punctuation now render; single line breaks inside a paragraph show as line breaks; collapsed thinking previews no longer show `**Title**` markers; bare URLs stop at CJK punctuation; `<br>` in tables becomes a line break and HTML comments are hidden.
- **Sub-agent reports and plans**: Task / Agent prompts and reports and ExitPlanMode plans render as Markdown, following the "Markdown / Raw" switch; command output and other tool results stay raw.
- **Codex sessions**: follow-up suggestions at the end of replies show as "↪" chips with the full prompt on hover; code-mode tool output shows as text instead of JSON with `\n` escapes.
- **Titles and outline**: session list titles, outline previews and external-session titles are plain-text summaries that skip Codex's injected context ("Files mentioned by the user", …) and show the actual request.
- **Local file links**: links to local paths are no longer dead links; they are dotted and show the path on hover.
- **Export**: tool-result headings include the tool name ("↩ Tool result · Bash"), so imported external sessions label results reliably.
