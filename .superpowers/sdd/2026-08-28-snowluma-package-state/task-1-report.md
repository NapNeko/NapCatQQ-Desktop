# Task 1 Report

## 修改文件

- `crates/ncd-domain/src/app_config.rs`
  - 为 `AppSettings` 增加可选 `snowluma_package` 字段，serde 名称为 `snowlumaPackage`，缺失时保持 `None`。
  - 增加 `full`、`lite` 与缺字段反序列化 round-trip 测试。
- `src-ui/core/ipc/generated/domain/AppSettings.ts`
  - 由 `ts-rs` 自动生成 `snowlumaPackage` 类型字段。

## 验证

- `cargo test -p ncd-domain app_settings_snowluma_package`：通过（2 passed）。
- `pnpm run ts-bindings`：通过；各导出测试通过，生成文件无额外非任务改动。
