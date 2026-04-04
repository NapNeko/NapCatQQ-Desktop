# 发布使用说明

本文档对应当前仓库的发布链路。

- 本地是唯一真相源：版本号、`docs/CHANGELOG.md`、`uv.lock` 都在本地生成并提交。
- CI 只负责基于已提交的代码构建 MSI 并发布 GitHub Release。
- 正式发布入口只有 `script/utils/release.py`。

如果你看到旧文档里还有“CI 自动回写版本文件”或“先打 tag 再补 CHANGELOG”的说法，以本文档为准。

## 一、发布前提

执行发布前，请先确认：

1. 工作区是干净的。
2. 目标版本号对应的 tag 还不存在。
3. 本地已经安装 `uv`。
4. 要发布的功能提交已经全部进入当前分支。

可先执行：

```powershell
git status --short
git tag -l v2.0.18
uv --version
```

## 二、两个脚本的职责

### 1. `script/utils/release.py`

唯一正式发布入口，负责完整流程：

1. 校验工作区是否干净
2. 解析目标版本号
3. 自动查找上一个语义化版本 tag
4. 收集 `上一个 tag..HEAD` 的提交
5. 过滤发布元数据提交
6. 更新 `pyproject.toml`
7. 更新 `src/core/config/__init__.py`
8. 更新 `docs/CHANGELOG.md`
9. 执行真实的 `uv lock`
10. 创建单个 release commit
11. 创建 `vX.Y.Z` tag
12. 可选推送分支和 tag

### 2. `script/utils/update_version.py`

辅助脚本，只同步版本文件，不提交、不打 tag、不推送。

适用场景：

- 想先预览这次发布会生成什么 `CHANGELOG`
- 想单独更新版本文件
- 调试 changelog 分类逻辑

## 三、标准发布流程

### 方式 A：本地生成 release，但先不推送

```powershell
python script/utils/release.py 2.0.18
```

执行完成后会得到：

- 一个新的 release commit：`chore(release): 发布 v2.0.18`
- 一个新的 tag：`v2.0.18`
- 已更新的：
  - `pyproject.toml`
  - `src/core/config/__init__.py`
  - `docs/CHANGELOG.md`
  - `uv.lock`

这个命令不会自动推送。

### 方式 B：本地生成 release 并直接推送

```powershell
python script/utils/release.py 2.0.18 --push
```

它会在本地完成 release commit 和 tag 后，再执行：

```powershell
git push origin <当前分支>
git push origin v2.0.18
```

tag 推送后，GitHub Actions 会自动：

1. 检出 tag 对应的提交
2. 构建 MSI
3. 上传构建产物
4. 使用仓库里的 `docs/CHANGELOG.md` 作为 Release body

CI 不会再修改仓库文件，也不会额外提交版本号。

## 四、预览和调试流程

如果你只想先看会生成什么版本文件，可以用：

```powershell
python script/utils/update_version.py 2.0.18
```

如果只想看 changelog 和版本替换效果，不想真的执行 `uv lock`，可以用：

```powershell
python script/utils/update_version.py 2.0.18 --no-lock
```

注意：

- `update_version.py` 会直接改工作区文件。
- 只是它不会帮你提交、打 tag 或推送。

## 五、手动指定起始 tag

默认情况下，脚本会自动查找“比目标版本更小的最近语义化版本 tag”作为起点。

如果你想强制指定比较范围，可以用：

```powershell
python script/utils/release.py 2.0.18 --from v2.0.16
```

或：

```powershell
python script/utils/update_version.py 2.0.18 --from v2.0.16
```

适用场景：

- 历史 tag 不连续
- 需要跳过某个错误发布
- 想临时修正 changelog 统计范围

## 六、CHANGELOG 机制

`docs/CHANGELOG.md` 现在使用固定锚点维护自动生成区域：

```md
<!-- BEGIN AUTO RELEASE NOTES -->
...
<!-- END AUTO RELEASE NOTES -->
```

发布脚本只会更新两部分：

1. 标题中的当前版本号
2. 自动生成区域中的发布说明

这样做的目的：

- 避免再用脆弱正则去猜整份文档结构
- 保留人工写的 Tips 和固定提醒
- 确保 GitHub Release body 与仓库中的文件完全一致

## 七、提交范围规则

发布说明的提交范围固定为：

```text
<previous_tag>..HEAD
```

同时会过滤这些不应该进入发布说明的提交：

- `chore(release): 发布 vX.Y.Z`
- `chore: release vX.Y.Z`
- `chore: 更新 napcatqq-desktop 版本至 X.Y.Z`
- Merge 提交

因此最终 changelog 面向“业务变更”，不会再把发布元数据本身算进去。

## 八、`uv.lock` 规则

发布脚本每次都会执行真实的：

```powershell
uv lock
```

这意味着：

- `uv.lock` 中项目版本会同步为最新版本
- 如果依赖解析结果有变化，也会一并被记录
- 不再出现“版本文件提交了，但 `uv.lock` 还残留未提交修改”的情况

如果 `uv lock` 失败，整个发布流程会直接失败，不会创建半成品 tag。

## 九、失败时如何处理

### 1. 提示“工作区存在未提交改动”

说明当前目录不是干净状态。

先处理现有修改，再执行发布：

```powershell
git status --short
```

### 2. 提示“目标 tag 已存在”

说明这个版本已经发过，或者历史上已经创建过同名 tag。

先检查：

```powershell
git tag -l v2.0.18
git show v2.0.18 --stat
```

当前脚本不会覆盖已有 tag，这是刻意设计，用来避免 tag 和最终提交再次脱节。

### 3. `uv lock` 失败

常见排查方式：

```powershell
uv lock
uv sync --frozen --no-dev
```

先把锁文件问题解决，再重新运行发布脚本。

## 十、推荐操作顺序

日常推荐按这个顺序发版：

```powershell
git pull
git status --short
python script/utils/update_version.py 2.0.18 --no-lock
python script/utils/release.py 2.0.18
git show --stat --decorate HEAD
git push origin <当前分支>
git push origin v2.0.18
```

如果你确认无误，也可以直接：

```powershell
python script/utils/release.py 2.0.18 --push
```

## 十一、相关文件

- 发布主入口：`script/utils/release.py`
- 版本同步辅助脚本：`script/utils/update_version.py`
- 共享发布逻辑：`script/utils/release_helpers.py`
- 发布说明文件：`docs/CHANGELOG.md`
- GitHub Release 工作流：`.github/workflows/release.yml`
- MSI 构建工作流：`.github/workflows/build-msi.yml`
