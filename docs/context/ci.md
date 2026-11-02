# CI 与发版耗时

目标是将缓存命中的常规 PR 门禁和 Desktop 发版分别压到约 5 分钟。冷缓存、Rust 工具链升级、依赖大改和 runner 排队另行记录；不靠跳过失败用例或缩短超时达标。新流程的实际耗时以合入后的 Actions 运行结果为准。

## 调整依据

| 运行 | 实测耗时 | 主要瓶颈 |
| :--- | :--- | :--- |
| [PR CI，2026-11-01](https://github.com/NapNeko/NapCatQQ-Desktop/actions/runs/41324166962) | 11 分 30 秒 | Windows core：普通账户步骤先编译整个 workspace，随后串行运行各 crate 测试 |
| [main CI，2026-11-02](https://github.com/NapNeko/NapCatQQ-Desktop/actions/runs/41456934015) | 8 分 20 秒 | Linux 先 check，再用不同 package 组合重复编译；Windows runtime 测试在 core 尾部 |
| [Desktop 发版，2026-11-02](https://github.com/NapNeko/NapCatQQ-Desktop/actions/runs/41458287094) | 21 分 18 秒 | MSI 构建步骤 18 分 40 秒，依赖缓存未命中，fat LTO 链接耗时长；Linux 构建步骤 7 分 45 秒 |

PR 中前端单测步骤为 3 分 27 秒。两端打包本来已经并行，继续拆发布编排无法消除 Windows 编译瓶颈。

## PR 门禁

- 前端按 Vitest 原生 `--shard=1/2`、`--shard=2/2` 分片，保留测试隔离。类型、版本、探针和 Python helper 检查只运行一次。默认单测不收集覆盖率。
- Windows core 与 runtime 并行；core 运行除 Tauri、runtime 外的全部 workspace 测试，runtime 运行自身全部测试。普通账户 Agent 用例仍执行真实非管理员进程，其编译参数与 core 一致。Tauri 保持独立 job。
- Linux 用一次 `cargo nextest run --workspace --all-targets` 编译全部目标，filterset 运行原有 host、Docker、安装回滚和 opener 契约。Windows 继续使用 `cargo test`，保留原有文档测试。
- Linux 同 Windows 一样复用编译产物，按内容恢复 checkout 输入时间戳；源码变化仍使对应 Cargo 指纹失效。缓存失效集成测试移到 Linux job，不再阻塞所有 job 的入口。
- 测试缓存按系统、Rust 工具链和编译参数隔离，不把 runner 镜像修订号作为失效条件，避免同批 Windows job 因镜像滚动更新而分别冷编译。旧 key 只作迁移回退。
- 保留路径过滤、IPC generated 差异检查和统一 Gate。矩阵任一分片失败都会使对应 job 失败；手动 smoke 保持显式选择，不能替代 PR 全量门禁。

## 发版

release profile 使用 `lto = false` 和 16 个 codegen units，保留 crate 内 ThinLTO、原有 `opt-level = "z"`、panic abort 与符号裁剪。缓存命中后跨 crate ThinLTO 的 Windows 应用编译仍需 5 分 44 秒，因此取消这一步整程序优化；包体积和运行性能需要与实际制品一起权衡。[Cargo profile 说明](https://doc.rust-lang.org/cargo/reference/profiles.html#lto)

[release-cache.yml](../../.github/workflows/release-cache.yml) 在 main 的 Rust / 构建配置变化后预热 Windows、Linux 的第三方依赖。精确命中时跳过编译；缺失时调用同一份构建工作流，不打包、不上传，也不继承发布 secrets。后续提交不打断正在进行的预热，避免反复冷启动。它独立于 PR Gate 和发版，亦可从 Actions 手动触发。

缓存只从 main 预热任务保存，tag 发版只读取。原因是不同 tag 不能互相读取缓存，但能读取默认分支缓存；依赖预热应在打 tag 前完成。[GitHub 缓存作用域](https://docs.github.com/en/actions/reference/workflows-and-actions/dependency-caching#restrictions-for-accessing-a-cache)

预热固定使用触发提交，缓存 key 显式包含根 `Cargo.toml` 的哈希。`rust-cache` 的成员清单不包含本仓库的虚拟 workspace 根清单；不补这一项，修改 release profile 后会误判为完整命中并跳过预热。

两端只缓存第三方编译产物。恢复后检查、保存前清理本仓库 crate、编译期代理常量和打包目录；含密钥的应用产物不会进入缓存。缓存检查或清理失败则 job 失败，不保存缓存。普通测试缓存使用未注入发布 secrets 的构建产物。

MSI 布局、UpgradeCode、AppImage 桌面集成、版本与发布校验保留。已压缩制品上传时不再二次压缩。

## 验收

合入后先等 main 的预热完成，再比较一次 Rust PR、一次前端 PR 和一次 Desktop 发版。记录总耗时、最慢 job、缓存是否命中及其恢复/保存耗时；冷构建单独记录。发布时启用 `run_gates` 仍会执行完整门禁，两端打包与门禁并行，发布等待全部成功。

本地维护入口为 `pnpm run test:ci`；更改缓存恢复逻辑时再设置 `CI_RUST_CACHE_TEST_CARGO=1` 执行真实 Cargo 失效测试。Linux nextest 命令应在 Ubuntu runner 验证，普通账户测试仅在可销毁的 GitHub Windows runner 执行。
