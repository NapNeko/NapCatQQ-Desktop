# 七个应用端框架配置审计（2026-10-22）

扫描以官方核心配置、默认文件、类型声明及 WebUI 元数据为依据。第三方插件和开放字典通过实例返回的 Schema、扩展键或原始配置入口承接，不将它们收窄为固定字段列表。

| 框架 | 本轮上游基准 | 覆盖与变更 |
| --- | --- | --- |
| Karin | 1.18.0，`2141ebab590acb10876f314948538ce7b191311a` | 核对 18 个系统环境变量与六份 JSON 配置，已有固定字段覆盖完整；保留开放 Redis 参数及未知 JSON 键。 |
| NoneBot2 | 2.5.0，`6e1efbea`；OneBot `2f5930d4` | 补齐两项核心超时和三项 OneBot 配置；增加 20 项驱动 / v12 按需预设，修复别名、大小写、URL 与 dotenv 语义。 |
| AstrBot | 4.29.1，`0cd42bd4dd216d8810da4e3f5fb2e381d3c6817e` | 增加按实例元数据与离线快照分组的高级设置，兼容新版运行器；高级设置与专页各自拥有写入路径。 |
| MaiBot | 1.3.14，`c602a2780f4f4dda4a6be46d67abc1cfb1ad420c` | 生成快照由 76 类 / 497 字段更新为 83 类 / 555 字段；bot 配置 8.15.37，model 配置 1.17.22，补齐新分区、模型任务与对象映射表单。 |
| Koishi | `97d5c395` | 全局和插件配置继续读取实例 Schema；补齐交叉结构中标签联合的必填校验。 |
| TRSS-Yunzai | 3.1.3，`29bf29ff` | 九份默认配置进入表单，补齐 bot、HTTPS、数据库、Milky、Satori 与 Redis 用户名；按实际加载方式提示重启。 |
| NeoBot | 2.0.2-a1 / 配置 0.8.0，`3f3b048d881190e69376777223718cec02b36edb` | 动态承接全部本体配置分区，补齐模型参数目录编辑，修复已有值显示与面板重启提示。 |

## 保存与兼容性

- dotenv 分别遵循 Node dotenv 与 python-dotenv 方言；未变值保留原文，JSON、引号和反斜线可稳定往返。NoneBot 的变量名、官方别名和覆盖文件按框架规则处理。
- MaiBot 与 Yunzai 在原 TOML / YAML 上按字段差量写回，保留注释和未知键。MaiBot 的旧字段迁移仅影响读取投影与用户实际修改，不自动升级配置版本；旧实例缺失模型文件时使用兼容默认模板。
- AstrBot 的高级配置按读取基线与草稿差量合入当前文档，避免替换受专页管理的父对象。子代理行携带额外字段。类型化入口目前只读取默认档案，写非默认档案会明确拒绝并保留草稿，防止误写。
- Koishi 和 NeoBot 使用实例 Schema 展示扩展配置；新增参数保留当前值与未知内容。复杂或未声明 Schema 的参数保留 JSON / 原始配置入口。
- 配置表单在外部刷新时保留未保存草稿；撤销、重读失败和实例切换均有回归覆盖。

## 复核资料

- [Karin / NoneBot2 字段与兼容审计](karin-nonebot2-2026-10-22.md)
- [MaiBot 字段、默认值与元数据差异](2026-10-22-maibot.json)
- [MaiBot 生成源记录](../../../src-ui/core/domain/apps/maibotSchema/upstream.json)
- [AstrBot 配置覆盖清单](../../../crates/ncd-appframework/src/astrbot/schema/coverage.json)
- [Koishi / Yunzai 审计](../../audits/2026-10-22-koishi-yunzai-config.json)
- [NeoBot 审计](../../audits/framework-config/2026-10-22/neobot.json)

## 再生成

对固定上游源码运行以下命令，再导出 IPC 类型并检查生成差异：

```sh
python scripts/maibot/codegen.py --src "$MAIBOT_SOURCE" --revision c602a2780f4f4dda4a6be46d67abc1cfb1ad420c
python scripts/astrbot/codegen.py --src "$ASTRBOT_SOURCE" --commit 0cd42bd4dd216d8810da4e3f5fb2e381d3c6817e --version 4.29.1
cargo test -p ncd-appframework export_bindings_ -- --test-threads=2
```

生成器仅加载受限配置声明或解释白名单 AST，不启动上游应用。运行时元数据优先于离线快照。
