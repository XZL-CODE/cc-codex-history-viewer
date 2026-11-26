## v0.17.0 更新内容 / What's new

- **Antigravity 显示估算成本**：Antigravity 与 Gemini CLI 的会话不再只统计 Token，按 Antigravity CLI 可选模型的 API 原价估算成本：Gemini 4 Neon、Carbon、Argon、Hydrogen 与 Gemini 3.8 Flash 按 Gemini API 标准价（不用限时优惠价），Claude 模型按 Claude 价格表。模型名后的推理档位（如 `-xhigh`）不影响计价。这是 API 等价估算，不代表 Google AI 订阅的实际支出；GPT-OSS 120B 等没有公开价格的模型不计入成本。
- **Claude 5.5 定价**：Claude Opus 5.5 有了估算成本，Claude Sonnet 5.5 的缓存读改按官方价（输入价的 0.05 倍）计。

- **Estimated cost for Antigravity**: Antigravity and Gemini CLI sessions now show an estimated cost instead of tokens only, at API list prices for the models Antigravity CLI offers: Gemini 4 Neon, Carbon, Argon, Hydrogen and Gemini 3.8 Flash at Gemini API standard prices (not introductory ones), and Claude models from the Claude price table. The reasoning effort appended to a model name (such as `-xhigh`) does not affect the price. This is an API-equivalent estimate, not actual Google AI subscription spend; GPT-OSS 120B and other models without a published price are excluded from cost.
- **Claude 5.5 pricing**: Claude Opus 5.5 now has an estimated cost, and Claude Sonnet 5.5 cache reads use the official price (0.05x input).
