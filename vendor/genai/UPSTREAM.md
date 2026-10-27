# rust-genai 0.7.1

本目录固定使用 crates.io 发布的 `genai 0.7.1`，上游为 [jeremychone/rust-genai](https://github.com/jeremychone/rust-genai)。SDK 只负责模型协议，不接管 NCD 会话、MCP 或审批。

- 发布包 SHA-256：`f031d61ad96cd018f8413af948613b9ec94cc0eff1dee670461aa490e933522c`。
- 许可证：MIT / Apache-2.0；原文见同目录许可证文件。
- 保存上游 `src/`、Cargo 清单、README、许可证、格式配置和库测试所需的两份 schema JSON。清单移除了未复制的示例和需要外部服务的集成测试 target；不自动执行上游真实模型用例。

NCD 补丁集中在以下边界：

1. `chat/stream_guard.rs` 定义应用提供的可失败校验接口，`ChatOptions` 和 client 将它接入请求、响应类型、原始帧和流结束。
2. `webc/stream_transport.rs` 在 SDK 缓冲前限制响应总字节数。启用 guard 时不读取失败 HTTP 响应体，保留状态码即可。
3. `webc/stream_budget.rs` 和 `webc/web_stream.rs` 限制跨 chunk 的帧大小；`event_source_stream.rs` 在 adapter 处理工具索引前调用帧校验。修正跨 chunk CRLF 的边界识别。
4. OpenAI、Responses、Anthropic、Gemini 路径不记录原始帧、模型错误正文或解析错误值。应用只接收固定类别错误，凭据 header 标记为敏感。
5. `openai/openai_model.rs` 的一处条件链换行适配本仓库使用的 rustfmt，清理 `zai/mod.rs` 文档行尾空格，无行为修改。
6. `rustls-tls-no-provider` 映射 reqwest 的同类 feature。NCD 为模型 client 单独配置既有 ring 和平台证书验证，避免新增 AWS-LC 本机构建，不修改进程默认加密 provider；SDK 上游默认 feature 保留。

NCD 的四协议校验和预算实现位于 `crates/ncd-runtime/src/assistant/builtin/model/sdk/`，完整结束、工具参数、私有推理与签名历史由该边界校验。SDK 不自行重试；NCD 注入的 HTTP client 也禁用重试和重定向。

升级时先比较发布包与本目录差异，再迁移上述补丁；不得仅更新版本号。需要运行 SDK 的本地 guard 测试、runtime/MCP 回归和绑定检查，验证截断、超限、取消、工具续轮与凭据脱敏。
