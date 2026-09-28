## v0.10.0 更新内容 / What's new

- **会话内 Token 用量明细**：对话详情页新增可展开的「Token 用量明细」面板：总量 / 成本 / 输入 / 输出 / 缓存命中率五张指标卡（附平均每次调用上下文、每轮平均成本、子代理占比、相比不用缓存节省多少），Token 与成本构成条，按调用顺序的堆叠柱图（点击跳到对应消息），多模型会话另有按模型拆分表。
- **逐调用与逐轮用量**：每条助手消息显示本次调用的上下文 / 输出 / 成本，悬停看完整拆分；resume/fork 复制进来、已计入原会话的调用会灰显标注；右侧大纲显示每个用户轮次的 Token 与成本。
- **缓存写分档计价**：解析 Claude Code 记录里缓存写的 5 分钟 / 1 小时分档，1 小时档按官方价（输入价 ×2）估算，首页与会话的成本都更准。
- **索引缓存 v6**：升级后首次启动会全量重建一次索引（大历史库需要等一会儿），之后仍是增量刷新。

- **In-session token usage details**: the conversation view gains a collapsible "Token usage details" panel: five tiles (total / cost / input / output / cache hit rate, with average context per call, average cost per turn, sub-agent share and the saving versus no caching), token and cost mix bars, a stacked per-call chart in call order (click a bar to jump to the message) and a per-model table for multi-model sessions.
- **Per-call and per-turn usage**: every assistant message shows the context / output / cost of its call, with the full split on hover; calls copied in by resume/fork that belong to another session are greyed out; the outline shows tokens and cost per user turn.
- **Cache-write tiers**: Claude Code's 5-minute / 1-hour cache-write split is now read and priced (1-hour at 2× the input rate), so session and overview costs are more accurate.
- **Index cache v6**: the first launch after upgrading rebuilds the index once (large histories take a moment); refreshes stay incremental afterwards.
