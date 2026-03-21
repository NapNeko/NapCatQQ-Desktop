# Desktop Update Layout

`update/` 用来维护 Desktop 自动更新的远端策略、迁移脚本和人工说明。

## 目录约定

```text
update/
  desktop_update_manifest.json
  README.md
  scripts/
    windows/
      migration_entry_template.bat
    common/
      verify_env.ps1
      backup_data.ps1
  docs/
    migration_template.md
```

## 职责划分

- `desktop_update_manifest.json`
  只放版本区间规则，不放长说明。
- `scripts/windows/*.bat`
  给客户端直接下载并执行的 Windows 入口脚本。
- `scripts/common/*.ps1`
  给维护者参考和复用的公共步骤模板。
- `docs/*.md`
  给开发者和用户阅读的迁移说明、回滚说明、手动恢复步骤。

## 关键约束

- 客户端当前一次只会下载并执行一个远端脚本。
- 所以 `manifest` 里的 `script_url` 必须指向一个可独立运行的入口脚本。
- 不要让入口 `.bat` 依赖仓库中相邻的 `.ps1` 文件存在于本地。
- `scripts/common/` 下的文件默认只是模板和维护参考，不是运行时依赖。

## manifest 维护规则

- `min_auto_update_version`
  低于这个版本的客户端直接禁止自动升级。
- `migrations[]`
  只描述需要迁移脚本的版本区间。
- 没命中任何 migration 的升级，默认走普通更新。
- `script_url`
  优先指向具体 tag，而不是长期指向 `main`。

示例：

```json
{
  "schema_version": 2,
  "min_auto_update_version": "v1.8.0",
  "migrations": [
    {
      "id": "cfg-layout-v2",
      "from_min": "v1.8.0",
      "from_max": "v1.9.99",
      "to_version": "v2.0.0",
      "strategy": "remote_script",
      "script_url": "https://raw.githubusercontent.com/NapNeko/NapCatQQ-Desktop/v2.0.0/update/scripts/windows/migrate_cfg_layout_v2.bat",
      "summary": "配置目录结构迁移"
    }
  ]
}
```

## 推荐流程

1. 先写迁移说明文档。
2. 再写自包含的入口脚本。
3. 本地验证备份、迁移、回滚三个路径。
4. 最后把规则写进 `desktop_update_manifest.json`。
