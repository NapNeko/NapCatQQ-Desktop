# 批量修复计划 - 6 个已知问题

> 创建时间：2026-06-11  
> 状态：待确认执行  
> 预计总工时：4-6 小时

---

## 问题清单与优先级

| # | 问题 | 优先级 | 复杂度 | 预计时间 |
|---|------|--------|--------|----------|
| 1 | Docker 安装后检测失败 | P0 | 中 | 30min |
| 2 | 任务队列完成任务无名称 | P1 | 低 | 15min |
| 3 | 任务队列完成后仍计时 | P1 | 低 | 已修复 ✅ |
| 4 | 任务日志输出太少 | P1 | 中 | 45min |
| 5 | 包管理器互斥锁 | P0 | 高 | 90min |
| 6 | 远端下载三重链路 | P2 | 高 | 120min+ |

---

## 问题 1: Docker 安装后检测失败

### 现象
用户反馈：Docker 安装完成后，组件页显示"未安装"，需要点击第二次安装才能检测到。

### 根因分析
1. 安装完成后前端缓存未刷新
2. `finalize_linux_docker_after_install` 的重试逻辑不够
3. 可能存在竞态：安装进程退出 vs daemon 就绪

### 定位代码
- **后端**：`crates/ncd-deploy/src/docker/install_progress.rs:383-430` - `finalize_linux_docker_after_install`
- **前端**：`src-ui/hooks/docker/useDockerHosts.ts` - 缓存刷新逻辑

### 修复方案
**方案 A（推荐）**：增加安装后等待时间
```rust
// finalize_linux_docker_after_install
for attempt in 0..6u32 {  // 从 4 改到 6
    if status.ready_to_deploy() { return Ok(status); }
    if !status.daemon_running {
        tokio::time::sleep(Duration::from_secs(5)).await;  // 从 3s 改到 5s
    }
}
```

**方案 B**：安装成功后前端主动刷新
```ts
// useDockerInstallProgressBridge
if (event.event.kind === 'finished' && event.event.ok) {
    queryClient.invalidateQueries({ queryKey: ['docker', 'hosts'] });
}
```

**推荐**：A + B 组合，后端延长等待 + 前端主动刷新

---

## 问题 2: 任务队列完成任务无名称

### 现象
任务执行完成后，任务列表中该任务显示空白标题

### 根因分析
可能原因：
1. `item.title` 为空字符串
2. `progress.message` 在 finished 时被清空
3. `buildTaskQueueSnapshot` 的标题生成逻辑有漏洞

### 定位代码
- `src-ui/core/domain/task-queue/buildSnapshot.ts:109-135` - `collectComponentActionItems`
- `src-ui/core/domain/task-queue/labels.ts` - 标题生成函数

### 修复方案
```ts
// buildSnapshot.ts
const title = componentId
    ? componentActionTitle(componentId, undefined, progress.message)
    : progress.message || taskId;  // ← 当 message 为空时 fallback 到 taskId

// 改为：
const title = componentId
    ? componentActionTitle(componentId, undefined, progress.message || '操作中')
    : progress.message || `任务 ${taskId.slice(0, 8)}`;
```

---

## 问题 3: 任务队列完成后仍计时 ✅

### 状态
**已在前一个会话修复**

### 修复内容
`TaskDetailPanel.tsx` 中时间显示逻辑改为：
- 如果任务已完成/失败/取消，显示固定的最终耗时
- 只有 running/pending 状态才实时计算

---

## 问题 4: 任务日志输出太少

### 现象
用户反馈：任务出错时日志不足，难以 debug

### 根因分析
1. 后端 `emit_log` 调用不足
2. 关键步骤（如命令执行、错误）没有记录
3. `progress.logs` 有 50 条上限（MAX_LOGS）

### 定位代码
- `crates/ncd-component/src/context.rs:93-111` - `emit_log` 定义
- `crates/ncd-deploy/src/docker/install_progress.rs` - Docker 安装日志
- `src-ui/core/domain/components/progress.ts:49` - MAX_LOGS = 50

### 修复方案

**后端增加日志点**（示例：Docker 安装）：
```rust
// install_progress.rs - 增加关键命令的日志
emit_log(&emit, ProgressLogLevel::Info, format!("执行命令: {}", cmd_summary));
emit_log(&emit, ProgressLogLevel::Warn, format!("命令失败: {}", err_detail));
```

**前端提高日志上限**：
```ts
// progress.ts
const MAX_LOGS = 200;  // 从 50 改到 200
```

**新增：日志导出功能**（可选）：
```tsx
// TaskDetailPanel.tsx
<Button onClick={() => exportLogs(progress.logs)}>
    导出日志
</Button>
```

---

## 问题 5: 包管理器互斥锁

### 现象
用户需求：
- 使用 apt/dnf 的任务（如安装 Docker、VNC）应该串行执行
- 第二个任务应进入队列状态，等待第一个完成

### 根因分析
**当前无全局互斥机制**：
- Docker 安装、组件安装都是并发的
- apt/dnf 并发执行会导致 dpkg lock 冲突

### 设计方案

**方案 A：全局包管理器锁（推荐）**

1. **后端**：在 `ncd-runtime` 增加全局锁
```rust
// crates/ncd-runtime/src/package_lock.rs (新文件)
pub struct PackageManagerLock {
    locks: Arc<RwLock<HashMap<String, Mutex<()>>>>,  // host_id -> lock
}

impl PackageManagerLock {
    pub async fn acquire(&self, host_id: &str) -> MutexGuard<()> {
        // 获取该主机的包管理器锁
    }
}
```

2. **集成点**：
- `docker::install_docker_with_progress` 开始前获取锁
- 组件安装（QQ、NodeJS）开始前获取锁
- 锁在安装完成后自动释放

3. **前端**：任务队列显示"等待包管理器"状态

**方案 B：任务队列串行化**

在前端任务队列层面实现：
- 检测任务是否需要包管理器
- 相同主机的包管理器任务串行调度

**推荐**：方案 A，后端强制互斥更可靠

---

## 问题 6: 远端下载三重链路

### 现象
用户需求：参考旧版实现"远程直接下载 + 镜像 + 本地兜底"三重链路

### 当前链路
**仅有"本地下载 → 上传"**：
1. Desktop 本地下载到 `/tmp`
2. 通过 SSH 上传到远端 `/tmp`
3. 远端解压安装

### 旧版 Python 实现（待调研）
需要查看：`.references/legacy-python/src/core/network/downloader.py`

### 设计方案

**三重链路逻辑**：
```
尝试 1: 远端直接下载（wget/curl）
  ↓ 失败
尝试 2: 远端从镜像下载
  ↓ 失败  
尝试 3: 本地下载 → 上传（当前唯一路径）
```

**实现难点**：
1. `Host` trait 没有 `download_url` 方法
2. 需要解析 wget/curl 的进度输出
3. 远端可能没有 wget/curl
4. 代理环境下远端直接下载可能失败

**实施步骤**：
1. 在 `Host` trait 增加 `download_url` 方法
2. 实现 `RemoteLinuxHost::download_url`（用 wget --progress=dot）
3. 修改 `QQComponent::install_linux` 等安装流程
4. 增加三重 fallback 逻辑

**预计改动量**：大（架构级）

---

## 执行建议

### 立即执行（低风险，高收益）
1. ✅ 问题 3：已修复
2. 问题 2：任务名称（15min）
3. 问题 4：日志量 - 仅提高 MAX_LOGS（5min）

### 需要确认后执行（中风险）
4. 问题 1：Docker 检测（30min）
5. 问题 4：后端增加日志点（45min）

### 需要单独规划（高风险，大改动）
6. 问题 5：包管理器互斥锁（90min，架构级）
7. 问题 6：三重下载链路（120min+，架构级）

---

## 验证计划

每个修复完成后：
1. `npm run typecheck` - TypeScript 检查
2. `cargo check -p ncd-*` - Rust 编译检查
3. 手动测试对应功能

---

## 下一步

**请用户确认**：
1. 是否立即执行"立即执行"部分（问题 2、4 部分）？
2. 问题 1、5 的修复方案是否认可？
3. 问题 6 是否作为独立 milestone，暂不在本次修复？
