<!-- 策展正文：只写用户向内容。安装/资源/支持由 scripts/release-notes.mjs 拼装。 -->
<!-- 预览: pnpm run release:notes:preview -- --kind watch --version 0.2.6 -->

**ncd-watch 0.2.6**

远端 Linux 主机侧监控二进制（Desktop 退出后仍可 Webhook / 探活）。

### 更新内容

#### 修复

- Docker 部署的 Bot 当 QQ 消息通知的发送方时，先用 `docker inspect` 查它的 OneBot HTTP 端口实际映射到宿主机哪里，不再只认 Desktop 按部署规则推出来的地址；自己用 compose 起、再导入 Desktop 的容器也能用

#### 改进

- QQ 消息通知按顺序试发送方，一个完全发不出去就换下一个；已经有人收到的不会换发送方重发
- 配合 Desktop 下一版：发送方的端口没映射到宿主机时，Desktop 不再写一个连不上的地址

与旧版 Desktop 兼容，没有新字段时行为和 0.2.5 一样。
