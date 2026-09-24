## v0.9.0 更新内容 / What's new

- **导入 Claude Code 会话**：顶栏新增「导入」，选择云端 Claude Code（claude.ai/code）打包的 zip 即可导入本机 projects 目录，之后与本机会话一样检索、查看和导出。先生成只读计划，逐会话判定新增 / 更新 / 跳过 / 冲突，冲突逐个选择「保留本机」或「用导入的覆盖」；只接受 `projects/<目录名>/` 之下的条目，绝不删除本机文件。
- **项目映射**：云端的 `/home/user/<仓库名>` 可映射到本机目录（按仓库名自动预填、可选文件夹、可保持原路径），只改写每行的 cwd，会话归到本机项目，本机 `claude --resume` 也能找到；映射会被记住，可在设置里删除。
- **对话详情**：`<persisted-output>` 标记的超大工具输出可按需展开完整内容，导出时内联；@ 引用或上传的文件以「附件」折叠块显示；只有 signature 的空思考块不再显示。

- **Import Claude Code sessions**: a new **Import** button in the top bar takes a zip packed by Claude Code on the web (claude.ai/code) and writes it into the local projects directory, after which the sessions are browsed, searched and exported like local ones. A read-only plan classifies each session as add / update / skip / conflict; conflicts are resolved one by one with "keep local" or "overwrite with import". Only entries below `projects/<dir>/` are accepted and nothing local is ever deleted.
- **Project mapping**: the cloud `/home/user/<repo>` can be mapped to a local folder (prefilled by repository name, chosen with a folder picker, or kept as is); only each line's cwd is rewritten, the session joins the local project and `claude --resume` finds it locally. Mappings are remembered and can be removed in Settings.
- **Conversation view**: tool outputs persisted as `<persisted-output>` expand to their full content on demand and are inlined in exports; files referenced with @ or uploaded show as a collapsed "Attachment" block; signature-only thinking blocks are hidden.
