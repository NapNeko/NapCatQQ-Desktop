# Docker 管理

Docker 页按远端主机提供容器、镜像、网络、数据卷四个视图。管理操作只要求 daemon 可用；Compose 插件仅用于部署。缺少 Compose 时仍可以查看和操作资源。

顶部「Docker 部署」携带当前主机跳到组件页的独立部署区。部署区负责安装或启动引擎、补全 Compose、准备框架镜像；「新建 Bot」预填主机、框架和 Docker 运行方式，容器在启动 Bot 时创建。返回 Docker 管理保留原主机。

- 容器：搜索、状态筛选、启动、停止、重启、暂停、恢复、日志与详情。详情展示退出码、OOM、健康状态、重启策略、网络、端口和挂载；运行中按需读取 CPU、内存和 I/O。
- 镜像：搜索、删除；没有 tag 的镜像按 ID 删除，避免误用仓库默认标签。
- 网络与卷：搜索、查看名称/驱动/作用域、删除。内置网络隐藏删除入口。卷删除必须输入完整名称，由 Docker 拒绝仍被引用的资源。
- 容器删除需要确认，默认不强制，不删除挂载卷；强制删除需单独勾选。
- 刷新失败展示真实错误和重试入口；自动刷新可选，页面隐藏时不后台轮询。写操作失败后回读状态，不自动重试写操作。
- 对话框绑定主机和容器 ID，切换主机后卸载；迟到响应不能覆盖当前主机。操作完成后只刷新发起操作的主机。

## 边界

这不是通用编排平台：不提供任意 Compose 编辑、批量 prune 或强制删除卷。协议 Bot 的安装和部署继续使用组件页和现有生命周期入口。

## 代码落点

- `crates/ncd-domain/src/docker/management.rs`：IPC 模型。
- `crates/ncd-deploy/src/docker/management.rs`：类型化 inspect 解析、资源管理与探测。
- `src-tauri/src/commands/docker/management.rs`：命令薄壳。
- `src-ui/hooks/docker/`：主机隔离、查询、操作与轮询。
- `src-ui/modules/docker/`：管理页和对话框。

探测优先当前 SSH 用户的 Docker context，仅在默认系统 socket 权限不足时回退 sudo，不能因 rootless daemon 不可达而悄悄切换到 root 的 daemon。inspect 只导出展示字段，不导出环境变量。

CLI 格式依据 [Docker inspect](https://docs.docker.com/reference/cli/docker/container/inspect/)、[stats](https://docs.docker.com/reference/cli/docker/container/stats/)、[network ls](https://docs.docker.com/reference/cli/docker/network/ls/) 和 [volume ls](https://docs.docker.com/reference/cli/docker/volume/ls/) 官方文档。

开发预览：启动 `pnpm dev` 后打开 `/dev/docker-preview.html`，使用内存模拟的两台主机，支持管理、部署区和预填新建 Bot 的导航。不连接实际 Docker，也不写入服务器配置。可用 `?status=missing`、`stopped`、`no-compose` 查看未安装、未启动和缺少 Compose 的状态。
