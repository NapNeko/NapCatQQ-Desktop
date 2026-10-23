# Docker 管理

Docker 页按远端主机提供容器、镜像、网络、数据卷四个视图。管理操作只要求 daemon 可用；Compose 插件仅用于部署。缺少 Compose 时仍可以查看和操作资源。

- 容器：搜索、状态筛选、启动、停止、重启、暂停、恢复、日志与详情。详情展示退出码、OOM、健康状态、重启策略、网络、端口和挂载；运行中按需读取 CPU、内存和 I/O。
- 镜像：搜索、删除；没有 tag 的镜像按 ID 删除，避免误用仓库默认标签。
- 网络与卷：搜索、查看名称/驱动/作用域、删除。内置网络隐藏删除入口。卷删除必须输入完整名称，由 Docker 拒绝仍被引用的资源。
- 容器删除需要确认，默认不强制，不删除挂载卷；强制删除需单独勾选。
- 刷新失败展示真实错误和重试入口；自动刷新可选，页面隐藏时不后台轮询。写操作失败后回读状态，不自动重试写操作。
- 对话框绑定主机和容器 ID，切换主机后卸载；迟到响应不能覆盖当前主机。操作完成后只刷新发起操作的主机。

## 边界

Docker 页不提供任意 Compose 编辑、批量 prune 或强制删除卷。协议 Bot 的镜像安装继续使用组件页；按 Bot 调整部署参数使用「Bot 配置 → 容器」，由现有生命周期应用。

## Bot 容器设置

NapCat / SnowLuma 的 Docker Bot 在配置中保存结构化的 `docker` 设置：`portMappings` 按容器端口覆盖自动映射或追加 TCP 端口，`environment` 追加环境变量，`restartPolicy` 设置 Docker 重启策略，`memoryMb` 设置内存上限，`publishOnAllInterfaces` 控制发布到 `127.0.0.1` 或 `0.0.0.0`，默认回环。旧配置无需迁移，后续创建容器默认使用 `unless-stopped` 重启策略；WebUI、QQ 账号、登录和指标所需环境变量由 Desktop 管理，不允许覆盖。

有效映射合并框架默认端口、已启用的 HTTP / SSE / WebSocket 服务端口和用户覆盖。宿主机端口默认使用账号偏移，界面预览完整映射并允许恢复自动值；反向连接不额外发布端口。Docker 派生配置将空地址或回环监听转换为容器内 `0.0.0.0`，持久化的连接配置保持原值。调试台按有效映射规划访问，watch 投递前读取容器的实际映射。

保存运行中的 Bot 时，部署参数有变化会更新 Compose 并重建容器，保留数据卷；停止的 Bot 在下次启动时应用。应用失败会报告错误，已保存的期望配置保留，可修正端口或主机问题后重试启动。发布端口属于容器创建参数，不能只执行 `docker restart`。

导入只读取现有网络配置并接管可识别的运行态，不代表任意外部 Compose 已成为 Desktop 托管项目。修改容器部署参数前需核对项目归属，不能用默认模板覆盖不兼容的挂载或未知服务。

## 代码落点

- `crates/ncd-domain/src/docker/management.rs`：IPC 模型。
- `crates/ncd-deploy/src/docker/management.rs`：类型化 inspect 解析、资源管理与探测。
- `src-tauri/src/commands/docker/management.rs`：命令薄壳。
- `src-ui/hooks/docker/`：主机隔离、查询、操作与轮询。
- `src-ui/modules/docker/`：管理页和对话框。
- `crates/ncd-domain/src/docker/bot_config.rs`：Bot 容器设置、有效映射与校验。
- `src-ui/modules/bot/config/next/DockerTab.tsx`：按 Bot 编辑端口、环境变量和运行设置。

使用说明见 [远端 Linux 管理](../user/remote-linux.md#调整-docker-bot-容器)。

探测优先当前 SSH 用户的 Docker context，仅在默认系统 socket 权限不足时回退 sudo，不能因 rootless daemon 不可达而悄悄切换到 root 的 daemon。inspect 只导出展示字段，不导出环境变量。

CLI 格式依据 [Docker inspect](https://docs.docker.com/reference/cli/docker/container/inspect/)、[stats](https://docs.docker.com/reference/cli/docker/container/stats/)、[network ls](https://docs.docker.com/reference/cli/docker/network/ls/) 和 [volume ls](https://docs.docker.com/reference/cli/docker/volume/ls/) 官方文档。

开发预览：启动 `pnpm dev` 后打开 `/dev/docker-preview.html`，使用内存模拟的两台主机，不连接实际 Docker，也不写入服务器配置。
