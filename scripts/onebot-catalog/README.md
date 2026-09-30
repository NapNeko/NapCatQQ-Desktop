# OneBot 动作目录快照

调试台的动作目录在连不上上游、或上游太旧没有调试接口时，用随桌面端打包的快照兜底。
快照由 `build-snapshots.mjs` 从上游产物生成，编进 `ncd-onebot` 二进制里。

## 重新生成

```bash
pnpm run catalog:snapshots
# 或指定输入
node scripts/onebot-catalog/build-snapshots.mjs --napcat <openapi.json> --snowluma <catalog.json>
```

| 参数 | 缺省 |
| --- | --- |
| `--napcat` | `D:/NapCat-Project/NapCatDocs/src/api/<最高版本>/openapi.json`（目录里按 semver 挑最高的，忽略 `index.md` 等非版本条目） |
| `--snowluma` | `.references/SnowLuma/packages/mcp/src/generated/catalog.json`，先找本仓库，再找主 checkout `D:/NapCat-Project/NapCatQQ-Desktop-V1` |

只依赖 Node 内置模块（Node >= 18）。

## 输出

`crates/ncd-onebot/src/catalog/snapshot/`：

- `napcat.json`：`{source, version, defs, actions}`。`defs` 是 OpenAPI 的 `components.schemas`
  （去掉 `x-schema-id`），动作里的 schema 用 `$ref: "#/components/schemas/X"` 引用它，
  由 Rust 侧的 `inline_refs` 在解析时展开。每个动作带 `description`（摘要）、`longDescription`
  （与摘要不同时才有）、`tags`、`payload`、`response`（`data` 的 schema）、`payloadExample`、
  `returnExample`、`errorExamples`。
- `snowluma.json`：`{source, version, actions, categories}`，`actions` 原样取自上游 `catalog.json`；
  `version` 是 SnowLuma 仓库当前提交的短 sha（不是 git 仓库时退回文件修改日期）。

生成结果是压缩 JSON，末尾一个换行。动作按名字排序，重新生成时 diff 只反映上游真实的变化。

## 转换里的取舍

- 返回示例只取动作自己写的。OpenAPI 里约 60 个动作的 `Success` 示例是引用共享的
  `Success_Default`（`data: {}`），那只是占位，这些动作实际多半返回 `null`，展示出来会误导，所以不收。
- `data` schema 只有 `description`、没有任何类型信息时按「没有返回值 schema」处理。
- 错误示例从 `Error_*` 引用（指向 `components.examples`）里解出 `{retcode, message}`。

## 何时重新生成

NapCat 或 SnowLuma 新增、改动了动作之后。重新生成后跑一遍 `cargo test -p ncd-onebot`：
`every_dangerous_name_exists_in_at_least_one_snapshot` 会指出被上游删掉的危险动作名。
