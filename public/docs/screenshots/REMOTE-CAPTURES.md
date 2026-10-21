# 远端教程截图来源

- 源码：`NapNeko/NapCatQQ-Desktop` 的 `main`，提交 `85cd7f40739d38a36c40d9d48ff5b390a2056cff`。
- 页面：原始软件前端静态预览 `http://127.0.0.1:1422`，Chromium，1440 × 1000，像素比 1。
- 明暗：偏好 `theme: auto`、`sidebarStyle: classic`、`motionEnabled: false`，在同一个表单分别模拟系统浅色和深色。
- `remote-create-light.png`、`remote-create-dark.png`：真实「添加远端服务器」表单，输入名称 `教程演示主机`、保留文档地址 `192.0.2.10`、用户名 `ubuntu`。密码为空，无真实 SSH 登录或凭据。

没有截图实际 QQ 二维码或已部署 Docker 容器：浏览器预览没有对应的服务器档案与 SnowLuma 登录模拟链路。正常 UI 无法到达这些状态，因此未改造软件、伪造主机、拼接界面或生成二维码。文档保留经 `main` 验证的原生操作步骤。具体阻断点见 `remote-manifest.json`。
