# Release CI 优化说明

本文档说明了 Release CI 的优化内容和使用方法。

## 优化内容

### 1. uv 环境管理

- ✅ 使用 `astral-sh/setup-uv@v4` action 管理 uv 工具
- ✅ 使用 `uv sync --frozen` 安装依赖，确保版本一致性
- ✅ 基于 `uv.lock` 文件锁定依赖版本

### 2. 智能缓存机制

- ✅ 缓存 uv 虚拟环境（`.venv` 目录）
- ✅ 缓存 key 基于 `uv.lock` 文件的哈希值
- ✅ 缓存回退策略：精确匹配 → 最近可用缓存
- ✅ 预期性能提升：依赖安装时间减少 40% 以上

### 3. 灵活的触发方式

支持 3 种触发方式：

1. **Tag 推送触发**（生产发布）
   ```bash
   git tag v1.7.9
   git push origin v1.7.9
   ```

2. **手动触发**（测试和调试）
   - 在 GitHub Actions 界面点击 "Run workflow"
   - 可选参数：
     - `version`: 版本号（例如：1.7.9 或 v1.7.9）
     - `skip_build`: 跳过构建，仅更新版本号

3. **分支推送触发**（CI 测试）
   ```bash
   git push origin release/test-ci
   ```

### 4. 版本号自动更新

自动从 git tag 提取版本号并同步更新到：
- `pyproject.toml`
- `src/core/config/__init__.py`
- `docs/CHANGELOG.md`

### 5. 自动生成更新日志

从 git commit 记录自动生成更新日志，支持的 commit 类型：

| Commit 前缀 | 分类 | 示例 |
|------------|------|------|
| `feat:` 或 `✨` | ✌️ 新增功能 | `feat: 添加用户管理功能` |
| `fix:` 或 `🐛` | 😭 修复功能 | `fix: 修复登录失败问题` |
| `perf:` 或 `⚡` | 😘 优化功能 | `perf: 优化启动速度` |
| `refactor:` 或 `♻️` | 😘 优化功能 | `refactor: 重构用户模块` |

## 使用方法

### 场景 1：正式发布新版本

1. 确保所有代码已提交到 main 分支
2. 创建并推送版本 tag：
   ```bash
   git tag v1.7.9
   git push origin v1.7.9
   ```
3. CI 会自动：
   - 更新三个文件中的版本号
   - 从 commit 记录生成更新日志
   - 构建 Debug 和 Release 版本
   - 创建 GitHub Release 并上传文件

### 场景 2：仅更新版本号（不构建）

1. 在 GitHub Actions 界面选择 "Build Release" workflow
2. 点击 "Run workflow"
3. 填写参数：
   - `version`: 1.7.9
   - `skip_build`: ✅ (勾选)
4. 运行后会更新版本号和更新日志，但不会构建

### 场景 3：测试 CI 流程

1. 创建测试分支：
   ```bash
   git checkout -b release/test-ci
   git push origin release/test-ci
   ```
2. CI 会运行完整流程（但不会创建 Release）

### 场景 4：手动运行完整构建

1. 在 GitHub Actions 界面选择 "Build Release" workflow
2. 点击 "Run workflow"
3. 不填写任何参数（或填写 version）
4. 运行后会执行完整构建流程

## 版本更新脚本使用

也可以在本地使用版本更新脚本：

```bash
# 更新到指定版本
python script/utils/update_version.py v1.7.9

# 或者不带 v 前缀
python script/utils/update_version.py 1.7.9
```

脚本会：
1. 从上一个 tag 到当前 HEAD 收集所有 commit
2. 按类型分类 commit（feat/fix/perf）
3. 更新 pyproject.toml、__init__.py、CHANGELOG.md
4. 显示更新日志预览

## Commit 消息规范

为了让自动生成的更新日志更有价值，建议遵循以下规范：

### 推荐格式

```
<type>: <description>

[optional body]

[optional footer]
```

### 示例

```bash
# 新增功能
git commit -m "feat: 添加用户头像上传功能"
git commit -m "✨ 支持自定义主题颜色"

# 修复 Bug
git commit -m "fix: 修复文件上传失败的问题"
git commit -m "🐛 解决启动时崩溃的 Bug"

# 性能优化
git commit -m "perf: 优化大文件加载速度"
git commit -m "⚡ 减少内存占用"

# 代码重构
git commit -m "refactor: 重构网络请求模块"
git commit -m "♻️ 简化配置文件结构"
```

## 缓存机制说明

### 缓存内容

- `.venv/` - uv 虚拟环境
- uv 的全局缓存（由 setup-uv action 自动管理）

### 缓存更新时机

当 `uv.lock` 文件内容发生变化时，缓存 key 会改变，触发重新安装依赖。

### 缓存回退

如果找不到精确匹配的缓存，会使用最近的可用缓存作为基础，只更新变化的依赖。

## 性能对比

### 优化前
- Python 设置：~30 秒
- 依赖安装：~5-8 分钟
- 总计：~5.5-8.5 分钟

### 优化后（有缓存）
- uv 设置：~10 秒
- Python 安装：~20 秒
- 依赖安装（缓存命中）：~30 秒
- 总计：~1 分钟

### 优化后（无缓存）
- uv 设置：~10 秒
- Python 安装：~20 秒
- 依赖安装（首次）：~3 分钟
- 总计：~3.5 分钟

**预期性能提升：80-90%（有缓存时）/ 40-50%（无缓存时）**

## 故障排查

### 问题 1：缓存未生效

**症状**：每次都重新安装所有依赖

**解决方法**：
1. 检查 `uv.lock` 文件是否存在
2. 检查 GitHub Actions 缓存限制（10GB）
3. 查看 workflow 日志中的缓存命中情况

### 问题 2：版本号更新失败

**症状**：版本号没有更新到预期值

**解决方法**：
1. 检查 tag 格式是否正确（v1.2.3）
2. 查看 workflow 日志中的版本提取步骤
3. 确认文件中有正确的版本号模式

### 问题 3：更新日志为空

**症状**：CHANGELOG.md 中没有生成内容

**解决方法**：
1. 检查 commit 消息是否符合规范（feat:/fix:/perf:）
2. 确认有上一个 tag 或足够的 commit 历史
3. 查看脚本输出的 commit 分类信息

## 参考资料

- [uv 文档](https://github.com/astral-sh/uv)
- [GitHub Actions 缓存](https://docs.github.com/en/actions/using-workflows/caching-dependencies-to-speed-up-workflows)
- [Conventional Commits](https://www.conventionalcommits.org/)
