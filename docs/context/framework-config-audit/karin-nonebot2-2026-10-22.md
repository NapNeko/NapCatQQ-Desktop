# Karin / NoneBot2 配置覆盖扫描（2026-10-22）

## 事实来源

- Karin（1.18.0）: `KarinJS/Karin@2141ebab590acb10876f314948538ce7b191311a`，`packages/core/src/types/config/{env,adapter,config,groups,privates,render,redis}.ts`、`packages/core/src/utils/config/default.ts`、`packages/web/src/components/config/system/env.tsx`。
- NoneBot2: `nonebot/nonebot2@6e1efbea0e0341b461c5ace5ecc794e302b3eac6`（2.5.0），`nonebot/config.py`、所有内置驱动 `nonebot/drivers/*.py`。
- OneBot: `nonebot/adapter-onebot@2f5930d4f6c7fb3a82cf3201639e8f0b0a6c124e`，`nonebot/adapters/onebot/{v11,v12}/config.py`。

## Karin

18 个 WebUI 系统环境键全部已有类型化读写与表单：
HTTP_ENABLE、HTTP_PORT、HTTP_HOST、HTTP_AUTH_KEY、WS_SERVER_AUTH_KEY、REDIS_ENABLE、PM2_RESTART、TSX_WATCH、LOG_LEVEL、LOG_DAYS_TO_KEEP、LOG_MAX_LOG_SIZE、LOG_FNC_COLOR、LOG_MAX_CONNECTIONS、FFMPEG_PATH、FFPROBE_PATH、FFPLAY_PATH、RUNTIME、NODE_ENV。

6 个 JSON 配置文件全部核对，无遗漏固定字段：
- config.json：master/admin，user.enable_list/disable_list；friend/group/directs/guilds/channels 各 enable/enable_list/disable_list/log_enable_list/log_disable_list。
- adapter.json：console.isLocal/token/host；onebot.ws_server.enable/timeout；ws_client[].enable/url/token；http_server[].enable/self_id/url/token/api_token/post_token。
- groups.json：key/inherit/cd/userCD/mode/alias/enable/disable/member_enable/member_disable。
- privates.json：key/inherit/cd/mode/alias/enable/disable。
- render.json：ws_server.enable；ws_client[].enable/url/token/isSnapka/reconnectTime/heartbeatTime；http_server[].enable/url/token/isSnapka。
- redis.json：上游采用开放 RedisClientOptions。常用 url/username/password/database 已有表单，其余通过原始文档编辑，flatten extra 保存时保留。

pm2.json 继续由桌面端进程托管策略排除，非本次漏项。任意插件环境键通过 custom 和原始文档承接；未知 JSON 字段通过 extra 无损保留。

## NoneBot2 缺口及修补

原先核心仅覆盖 8 项：DRIVER/HOST/PORT/LOG_LEVEL/SUPERUSERS/NICKNAME/COMMAND_START/COMMAND_SEP；新增 API_TIMEOUT、SESSION_EXPIRE_TIMEOUT，使 Config 中 10 项全部有表单。
ENVIRONMENT 属于 Env，在 .env 原始文档编辑；已有文件布局跟随其选择 .env.{ENVIRONMENT}。

OneBot v11 原先仅 ACCESS_TOKEN 有表单，补齐 SECRET、WS_URLS、API_ROOTS，并识别官方 ONEBOT_V11_* 四个别名，按官方 alias 优先读取，保存/重新对接同步 v11 专用别名与通用键（含 base 中有别名、overlay 中只有通用键的情况）。支持 HTTP API self_id→URL 的 JSON 字典。错误 JSON 保留为原始字段并给出校验，不会静默变成空字典。

内置驱动完整扫描结果：仅 FastAPI 和 Quart 定义附加 Config；aiohttp/httpx/httpx2/websockets/none 未定义独立配置项。以下 16 项在驱动与自定义键中提供可选择的预设，按需加入，不会为未使用的驱动自动写入：
- FastAPI：FASTAPI_OPENAPI_URL、FASTAPI_DOCS_URL、FASTAPI_REDOC_URL、FASTAPI_INCLUDE_ADAPTER_SCHEMA、FASTAPI_RELOAD、FASTAPI_RELOAD_DIRS、FASTAPI_RELOAD_DELAY、FASTAPI_RELOAD_INCLUDES、FASTAPI_RELOAD_EXCLUDES、FASTAPI_EXTRA。
- Quart：QUART_RELOAD、QUART_RELOAD_DIRS、QUART_RELOAD_DELAY、QUART_RELOAD_INCLUDES、QUART_RELOAD_EXCLUDES、QUART_EXTRA。

OneBot v12 的 4 个独立命名空间键也提供按需预设：ONEBOT_V12_ACCESS_TOKEN、ONEBOT_V12_WS_URLS、ONEBOT_V12_API_ROOTS、ONEBOT_V12_USE_MSGPACK。它们不与 ncd 的 OneBot v11 对接配置合并。其他适配器/插件无限开放配置继续由 custom 与原始文档承接。

默认值校正：配置缺失时 COMMAND_START 按官方 ["/"]，COMMAND_SEP 按官方 ["."]；已有显式配置不变。LOG_LEVEL 可填写官方支持的名称或数字等级。API_TIMEOUT 保留数值文本，SESSION_EXPIRE_TIMEOUT 接受十进制秒数、HH:MM:SS、days 前缀与 ISO 8601（含负数和微秒），其中数字秒数和无歧义 clock 按整数微秒转为兼容 ISO；已合法的 ISO 仅校验并保留原文（trim），保留实例自身 Pydantic 的 submicro 解释。Pydantic v2 不接受数字秒数字符串，因此如输入120会写成PT120S；缺省为PT2M。

读写兼容：NoneBot2 dotenv 全部使用 Python 方言；overlay 按大小写不敏感规则覆盖 base；JSON 数组/字典可往返；自定义键与注释保留，重复键和系统键冲突可在保存前定位。

## 验证计划

Rust NoneBot2 局部测试覆盖核心与 OneBot 字段往返、别名优先与重新对接、未知插件键及注释保留、官方缺省值、大小写 overlay、错误 JSON/URL/非有限超时。前端局部测试覆盖默认值、路径校验、20 项可选预设、不覆盖既有设置及自定义键冲突。发布前需执行真实 typecheck / Rust 检查 / 绑定导出；环境尚不可用时不得声称已验证。

追加审查：OneBot URL 使用 URL 真解析检查协议和主机，接受大写协议，拒绝空主机；自定义环境键拒绝首尾空白，避免保留键绕过。安装/对接写入使用大小写不敏感更新，保留已有客户端驱动并去重端口。

负数时长边界：Pydantic v1 与 v2 对负数 day/clock 的解释不同（例如 `-1 day, 02:03:04`）。配置表单保留原字段并拒绝保存这种歧义写法，明确提示改用负数秒数或 `-P` 开头的 ISO 时长，避免静默改变既有设置含义；`-00:02:00` 同样拒绝。负数秒数与负数 ISO 保持支持。stdlib `timedelta(days=-1,hours=2,minutes=3,seconds=4,microseconds=5)` 等于 `-timedelta(seconds=79015,microseconds=999995)`，不是负26小时。
