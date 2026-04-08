# 远程 SSH 方案推进进度

## 文档说明

本文件用于记录 NapCatQQ Desktop 的“本地 Desktop + 远程 Linux SSH 直连”方案推进状态。

相关规划文档见 [`docs/general/remote_ssh_plan.md`](docs/general/remote_ssh_plan.md)。

---

## 当前阶段

当前处于：**P1 MVP 集成阶段（进行中）**

总体判断：

- P0 基础设施层已经完成
- P1 已经启动，并完成了第一轮骨架串接
- 当前主线任务是把“配置导出 → 上传 → 解包/部署 → 状态写回 → UI 操作”真正串成闭环

---

## 已完成事项

### P0：基础设施层

- [x] 远程连接模型、SSH 凭据模型、最小安全约束
- [x] 远程 SSH 客户端与错误模型
- [x] 远程执行抽象层
- [x] Linux Core 部署骨架
- [x] 远端状态 / 日志骨架
- [x] 远程配置入口与方案规划文档

对应代码：

- [`src/core/remote/ssh_client.py`](src/core/remote/ssh_client.py)
- [`src/core/remote/models.py`](src/core/remote/models.py)
- [`src/core/remote/errors.py`](src/core/remote/errors.py)
- [`src/core/remote/execution_backend.py`](src/core/remote/execution_backend.py)
- [`src/core/remote/deployment.py`](src/core/remote/deployment.py)
- [`src/core/remote/status.py`](src/core/remote/status.py)
- [`src/core/remote/remote_manager.py`](src/core/remote/remote_manager.py)

### P1：已完成的部分

- [x] 打通本地配置导出到远端上传的第一步
- [x] 补上远端配置解包与部署脚本执行的第一版骨架
- [x] 明确最小 `status.json` 结构，并接通启动 / 停止后的状态写回骨架
- [x] 在设置页补齐第一版“初始化工作区 / 环境探测 / 连接测试”交互
- [x] 将远程功能从弹窗改为设置页独立标签页
- [x] 修复远程功能接入过程中的循环导入问题
- [x] 修复远程页使用不存在的 `FluentIcon` 枚举导致的崩溃问题

对应代码与资源：

- [`src/core/config/config_export.py`](src/core/config/config_export.py)
- [`src/core/remote/deployment.py`](src/core/remote/deployment.py)
- [`src/core/remote/status.py`](src/core/remote/status.py)
- [`src/core/remote/templates.py`](src/core/remote/templates.py)
- [`src/resource/script/remote_deploy_napcat.sh`](src/resource/script/remote_deploy_napcat.sh)
- [`src/ui/page/setup_page/sub_page/remote.py`](src/ui/page/setup_page/sub_page/remote.py)
- [`src/ui/page/setup_page/__init__.py`](src/ui/page/setup_page/__init__.py)

---

## 当前进行中

### P1 主线任务

- [-] 定义并落地远端部署脚本协议，明确安装输入、输出、返回码与幂等策略

当前状态：

- 已有第一版部署脚本模板
- 已有本地配置导出并上传到远端的能力
- 已有运行状态文件的最小落点
- 还缺完整的部署协议固化与 UI 闭环串接

---

## 下一步

### 高优先级

- [ ] 继续补齐“部署”交互
  - 上传配置包
  - 上传部署脚本
  - 执行部署脚本
  - 展示部署结果

- [ ] 继续补齐“运行控制”交互
  - 启动
  - 停止
  - 重启
  - 状态刷新

- [ ] 继续补齐“日志查看”交互
  - 读取远端日志尾部
  - 手动刷新
  - 展示错误信息

### 中优先级

- [ ] 细化远端部署脚本协议
  - 明确返回码
  - 明确错误输出格式
  - 明确幂等规则
  - 明确失败后的残留处理方式

- [ ] 补测试与失败回滚策略
  - 确保本地模式不受远程模式影响
  - 确保配置上传 / 解包 / 部署失败时有清晰反馈

---

## 阶段判断

### 已完成

- P0：**已完成**

### 当前阶段

- P1：**进行中**

### 尚未进入

- P2：增强版本（配置回读、版本检查、多服务器管理等）
- P3：高级能力（实时推送、轻量 helper、批量编排等）

---

## 一句话总结

当前项目已经从“远程 SSH 想法验证”进入“P1 MVP 实做阶段”，基础设施已具备，现阶段重点是把 [`src/core/remote/deployment.py`](src/core/remote/deployment.py)、[`src/core/remote/status.py`](src/core/remote/status.py) 与 [`src/ui/page/setup_page/sub_page/remote.py`](src/ui/page/setup_page/sub_page/remote.py) 串成真正可用的部署、启停和日志闭环。
