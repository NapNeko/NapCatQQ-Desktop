# HTTP 下载与临时状态

入口在 `crates/ncd-network`。聊天文件、组件单链接走 `download_url`；组件镜像和 Desktop 更新走 `download_smart`。远端主机自行运行的 wget/curl，以及 SFTP 传输，不经过此下载器。

## 调度

- 单链接先请求 `bytes=0-0`。返回 200 时直接消费该响应，不为普通服务器重复下载探测数据。
- 镜像沿用有限字节的吞吐测速，选定一个源后分片，不混合多个镜像的字节。
- 默认从 4 MiB 开始并行，单文件最多 8 个 worker，可配置上限 16。所有任务的分片 HTTP 请求共用 16 个许可；探测和单流不占这份分片预算。
- worker 动态领取片段。常规片段为 1–8 MiB，文件较小时按并发数缩小。较快连接继续领取剩余片段，减少固定等分的尾部等待。
- 独立文件句柄定位写入同一临时文件，256 KiB 写缓冲；没有分片完成后的整文件合并拷贝。
- 断流保留已写入偏移，只重取剩余后缀。每片最多 3 次请求，指数退避；429/502/503/504 可重试，尊重最多 10 秒的数值 Retry-After。等待许可、请求和退避都响应取消。

连接数不能突破服务端总限速或用户线路上限；本地限速测试验证调度收益，不代表真实公网峰值。

## 一致性与提交

只有强 ETag 或有效完整 SHA256 才允许分片；否则退回单流。每个响应核对起止偏移、总大小、长度、编码和已固定的 ETag。If-Range 返回 200 或表示发生变化时停止所有 worker，再从完整响应重下。镜像失败可切换备用源，字节不跨源续接。

每次调用在目标文件旁创建独占 `.ncd-download-<pid>-<id>` 目录。成功完成且散列匹配后才 rename 到目标；失败、取消和校验错误都保留原目标。并发 worker 持有临时目录所有权，回退之前必须排空 writer；最后一个所有者释放时清理自己的临时文件和目录。强制终止进程可能留下目录，下一次下载不复用或拼接这些字节。

不引入持久化分片缓存或无来源凭据的跨调用续传。HTTP 客户端按一次下载固定代理配置快照；在途任务结束后释放旧快照。连接池每主机最多保留 16 条空闲连接，60 秒空闲过期。进度由聚合器按 250 ms 汇总，避免每个 worker 都向 UI 高频推送。

## 验证与参考

`crates/ncd-network/tests/parallel_download.rs` 覆盖动态调度、剩余后缀重试、版本变化、降级、原文件保护、取消和并发预算。忽略的 `local_throttled_throughput_comparison` 提供同一受控服务器下单流与 8 路的对比。

2026-10-17 Windows Release 本地对照：32 MiB 文件、每连接每 4 ms 发送 64 KiB，单流 2.293 s / 13.96 MiB/s，8 路 0.298 s / 107.30 MiB/s，7.69 倍。该用例不设置易受机器负载影响的速度断言；11 个分片行为用例、15 个既有自适应下载用例、39 个网络单测和 11 个组件下载相关用例全部通过。

设计参考 [aria2 分片及连接限制](https://aria2.github.io/manual/en/html/aria2c.html)、[aria2 SegmentMan](https://github.com/aria2/aria2/blob/master/src/SegmentMan.cc)、[hf_transfer 的有界并发和定位写入](https://github.com/huggingface/hf_transfer/blob/main/src/lib.rs) 与 [RFC 9110 Range / If-Range](https://www.rfc-editor.org/rfc/rfc9110.html#name-if-range)。实现使用现有 Rust/Tokio/reqwest 依赖，未复制外部下载器源码。
