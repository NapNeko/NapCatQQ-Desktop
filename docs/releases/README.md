# 发版说明（策展正文）

本目录存放 **GitHub Release 的用户向更新内容**。  
安装步骤、资源表、支持渠道由 `scripts/release-notes.mjs` 在预览/发版时自动拼装，**不要**在策展文件里重复写整页模板。

## 文件命名

| 产物 | 文件 | 对应 tag |
| :--- | :--- | :--- |
| Desktop MSI | `vX.Y.Z.md` | `vX.Y.Z` |
| ncd-watch | `watch-vX.Y.Z.md` | `watch-vX.Y.Z` |

示例：`v3.0.1.md`、`watch-v0.2.6.md`。

## 本地流程（推荐）

```bash
# 1. 从提交生成草稿（已存在则拒绝覆盖，除非 --force）
#    默认读仓库根 .env 的 OPENAI_* / OPENROUTER_* 调模型（对齐旧版）；
#    无 key 或加 --no-ai 则规则归类
pnpm run release:notes:draft -- --version 3.0.1

# 2. 编辑 docs/releases/v3.0.1.md —— 只留用户能感知的条目
#    删掉 chore/内部重构；改写成「结果」而不是 commit subject

# 3. 预览完整 GitHub 正文（含安装/资源/页脚）
pnpm run release:notes:preview -- --version 3.0.1

# watch 同理
pnpm run release:notes:draft -- --kind watch --version 0.2.6
pnpm run release:notes:preview -- --kind watch --version 0.2.6
```

### AI 配置（可选）

在仓库根 `.env`（已 gitignore）增加，与旧版 `script/utils/.env` 字段兼容：

```ini
OPENAI_API_KEY="sk-or-v1-..."
# 或 OPENROUTER_API_KEY="sk-or-v1-..."
OPENAI_API_URL="https://openrouter.ai/api/v1/chat/completions"
OPENAI_MODEL="z-ai/glm-4.5-air:free"
```

模板见根目录 `.env.example`。

满意后把 `docs/releases/vX.Y.Z.md` **提交进仓库**，再打 tag / 跑 Release 工作流。  
CI 会优先读取本目录文件；没有策展文件时才用自动草稿（并在页脚提示建议策展）。

## 一键发版（推荐，对齐旧版 release.py）

```bash
# 工作区干净时：写版本 + 策展 + 单 commit + 本地 tag（默认不 push）
pnpm run release -- 3.1.2

# 确认后推送分支与 tag → 触发 Release MSI
pnpm run release -- 3.1.2 --push

# 跳过交互确认
pnpm run release -- 3.1.2 --yes --push
```

无 `docs/releases/vX.Y.Z.md` 时会自动 draft；仍建议先改策展再发。  
分步工具仍可用：`release:bump` / `release:prepare` / `release:notes:*`。

## 策展文件写什么

**写：**

- 一句话版本定位（可选加粗标题句）
- `### 更新内容` 下的 **新增 / 修复 / 改进 / 安全 / 破坏性变更**
- 仅在必要时的迁移提示、已知限制

**不写（脚本会拼）：**

- `### 安装` / `### 升级说明` / `### 资源` / `### 支持`
- 完整 `git log` 列表
- `latest.json` / 工作流链接等实现细节

## 文案约定

1. **中文为主**，专有名词保留英文（MSI、SSH、WebUI、ncd-watch）。
2. **一条 bullet = 一个用户结果**，少写模块路径与 `fix(ui):` 前缀。
3. 热修（patch）建议 3～8 条；大版本可加 Highlights，仍避免 dump 全部提交。
4. 内部 `chore` / `ci` / `test` / `style` 默认不要进正文。
5. 需要给开发者看的完整 diff：页脚会带 compare 链接即可。

## 最小模板

```markdown
<!-- 策展正文：只写用户向内容。安装/资源/支持由 scripts/release-notes.mjs 拼装。 -->

**NapCatQQ Desktop x.y.z**

一句话说明本版重点。

### 更新内容

#### 修复

- …

#### 改进

- …
```

## CI 行为

- `release.yml`（Desktop `v*.*.*`）与 `release-ncd-watch.yml`（`watch-v*`）在 publish 阶段调用：

  `node scripts/release-notes.mjs render --kind … --version … --out release-notes.md`

- 若对应策展文件已提交：Release 正文 = 策展 + 标准壳。
- 若未提交：自动归类草稿 + 页脚提示「建议发版前策展」。
- 手动 `workflow_dispatch` 默认仍可出 draft release，便于在 GitHub 上再改一版。
