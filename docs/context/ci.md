# CI 与发版耗时

目标是将缓存命中的常规 PR 门禁和 Desktop 发版分别压到约 5 分钟。冷缓存、Rust 工具链升级、依赖大改和 runner 排队另行记录；不靠跳过失败用例或缩短超时达标。本轮全量 CI 已达到目标，Desktop 发版仍有差距，实测见下方验收记录。

## 调整依据

| 运行 | 实测耗时 | 主要瓶颈 |
| :--- | :--- | :--- |
| [PR CI，2026-11-01](https://github.com/NapNeko/NapCatQQ-Desktop/actions/runs/41324166962) | 11 分 30 秒 | Windows core：普通账户步骤先编译整个 workspace，随后串行运行各 crate 测试 |
| [main CI，2026-11-02](https://github.com/NapNeko/NapCatQQ-Desktop/actions/runs/41456934015) | 8 分 20 秒 | Linux 先 check，再用不同 package 组合重复编译；Windows runtime 测试在 core 尾部 |
| [Desktop 发版，2026-11-02](https://github.com/NapNeko/NapCatQQ-Desktop/actions/runs/41458287094) | 21 分 18 秒 | MSI 构建步骤 18 分 40 秒，依赖缓存未命中，fat LTO 链接耗时长；Linux 构建步骤 7 分 45 秒 |

PR 中前端单测步骤为 3 分 27 秒。两端打包本来已经并行，继续拆发布编排无法消除 Windows 编译瓶颈。

## PR 门禁

- 前端按 Vitest 原生 `--shard=1/2`、`--shard=2/2` 分片，保留测试隔离。类型、版本、探针和 Python helper 检查只运行一次。默认单测不收集覆盖率。
- Windows core 与 runtime 并行；core 运行除 Tauri、runtime、MCP 外的全部 workspace 测试，runtime 组运行 runtime 与依赖它的 MCP 测试，避免两组重复编译 runtime。普通账户 Agent 用例仍执行真实非管理员进程，其编译参数与 core 一致。Tauri 保持独立 job。
- Linux 用一次 `cargo nextest run --workspace --all-targets` 编译全部目标，filterset 运行原有 host、Docker、安装回滚和 opener 契约。Windows 继续使用 `cargo test`，保留原有文档测试。
- Linux 同 Windows 一样复用编译产物，按内容恢复 checkout 输入时间戳；源码变化仍使对应 Cargo 指纹失效。缓存失效集成测试移到 Linux job，不再阻塞所有 job 的入口。
- 测试缓存按系统、Rust 工具链和编译参数隔离，不把 runner 镜像修订号作为失效条件，避免同批 Windows job 因镜像滚动更新而分别冷编译。旧 key 只作迁移回退。
- 保留路径过滤、IPC generated 差异检查和统一 Gate。矩阵任一分片失败都会使对应 job 失败；手动 smoke 保持显式选择，不能替代 PR 全量门禁。

## 发版

release profile 使用 `lto = false` 和 16 个 codegen units，保留 crate 内 ThinLTO、原有 `opt-level = "z"`、panic abort 与符号裁剪，移除跨 crate LTO。此前 [ThinLTO 快照的发布演练](https://github.com/NapNeko/NapCatQQ-Desktop/actions/runs/41497764514)为 10 分 59 秒；本次源码和 runner 不同，尚不能证明这项参数调整的独立提速效果。[Cargo profile 说明](https://doc.rust-lang.org/cargo/reference/profiles.html#lto)

[release-cache.yml](../../.github/workflows/release-cache.yml) 在 main 的 Rust / 构建配置变化后预热 Windows、Linux 的第三方依赖。精确命中时跳过编译；缺失时调用同一份构建工作流，不打包、不上传，也不继承发布 secrets。后续提交不打断正在进行的预热，避免反复冷启动。它独立于 PR Gate 和发版，亦可从 Actions 手动触发。

缓存只从 main 预热任务保存，tag 发版只读取。原因是不同 tag 不能互相读取缓存，但能读取默认分支缓存；依赖预热应在打 tag 前完成。[GitHub 缓存作用域](https://docs.github.com/en/actions/reference/workflows-and-actions/dependency-caching#restrictions-for-accessing-a-cache)

预热固定使用触发提交，缓存 key 显式包含根 `Cargo.toml` 的哈希。`rust-cache` 的成员清单不包含本仓库的虚拟 workspace 根清单；不补这一项，修改 release profile 后会误判为完整命中并跳过预热。

两端只缓存第三方编译产物。恢复后检查、保存前清理本仓库 crate、编译期代理常量和打包目录；含密钥的应用产物不会进入缓存。缓存检查或清理失败则 job 失败，不保存缓存。普通测试缓存使用未注入发布 secrets 的构建产物。

MSI 布局、UpgradeCode、AppImage 桌面集成、版本与发布校验保留。已压缩制品上传时不再二次压缩。

## 验收（2026-11-03）

最终构建配置和源码为 `ff334a01`。全量套件在 main 上手动运行；打包使用 `dry_run=true`、`run_gates=false`，同一提交的完整 CI 已通过。以下均为 Actions 实测，未新建 PR 或发布正式版本。

| 运行 | 耗时 | 条件与结果 |
| :--- | :--- | :--- |
| [全量 CI](https://github.com/NapNeko/NapCatQQ-Desktop/actions/runs/41505237713) | **4 分 05 秒** | 缓存命中，全绿；core 3 分 32 秒、runtime/MCP 3 分 34 秒、Tauri 2 分 07 秒、Linux 1 分 17 秒；前端两片 2 分 40 秒 / 1 分 03 秒 |
| [分组切换后的首次 CI](https://github.com/NapNeko/NapCatQQ-Desktop/actions/runs/41503833645) | 7 分 44 秒 | 全绿；core 的依赖组合变化，首次重新编译用了 3 分 11 秒 |
| [Desktop 打包演练](https://github.com/NapNeko/NapCatQQ-Desktop/actions/runs/41505742187) | **11 分 45 秒** | 两端依赖缓存精确命中；MSI job 11 分 27 秒、Linux job 7 分 14 秒，制品校验通过；不含正式发布上传 |
| [新 profile 首次依赖预热](https://github.com/NapNeko/NapCatQQ-Desktop/actions/runs/41502435339) | Windows 19 分 46 秒 / Linux 8 分 41 秒 | 冷构建，独立于 PR 门禁和发版；后续相同依赖与配置复用缓存 |

发布尚未达到 5 分钟。Windows 依赖恢复耗时 34 秒，Agent 编译 2 分 12 秒、桌面程序编译 6 分 31 秒，两者串行；Linux 应用编译 3 分 40 秒，随后打包约 1 分 53 秒。后续应针对应用 crate 编译和 runner 性能继续优化。

本次 MSI 为 17,170,432 字节，UpgradeCode 与布局检查通过，Linux AppImage 桌面集成检查通过。此前 ThinLTO 快照 MSI 为 17,002,496 字节；两次源码不同，这些数据不能隔离 LTO 对体积和性能的影响。本轮未进行应用运行性能基准测试。

后续比较实际 Rust PR、前端 PR 与发版时，继续记录队列、编译和缓存耗时。启用 `run_gates` 仍会执行完整门禁，两端打包与门禁并行，发布等待全部成功。

本地维护入口为 `pnpm run test:ci`；更改缓存恢复逻辑时再设置 `CI_RUST_CACHE_TEST_CARGO=1` 执行真实 Cargo 失效测试。Linux nextest 命令应在 Ubuntu runner 验证，普通账户测试仅在可销毁的 GitHub Windows runner 执行。
