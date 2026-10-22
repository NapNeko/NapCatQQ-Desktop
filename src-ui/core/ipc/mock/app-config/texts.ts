// 各框架原始文件 Tab 的假文本样本：JSON.stringify 的样本值保持与拆分前逐字节一致。

/// AstrBot 插件配置缺文件时后端按 schema 物化默认值；mock 里同一份形状。
export const ASTRBOT_PLUGIN_DEFAULTS = `${JSON.stringify(
    {
        token: '',
        mode: 'chat',
        prompt: '',
        enabled_groups: [],
        limits: { per_user: 20, strict: false },
        extra: {},
    },
    null,
    2,
)}\n`;

export const YUNZAI_PLUGIN_TEXT = '# 插件自己的配置，改完插件一般会自己重新读\nenable: true\n';

export const KOISHI_TEXT: Record<string, string> = {
    koishi: 'plugins:\n  group:server:\n    server:cj4vi7:\n      port: 23140\n      host: 127.0.0.1\n',
    env: 'GITHUB_MIRROR = https://ghproxy.com/https://github.com\n',
    package: '{\n  "name": "@koishijs/boilerplate",\n  "version": "1.16.0"\n}\n',
};

export const MAIBOT_TEXT: Record<string, string> = {
    bot_config:
        '[inner]\nversion = "8.15.37"\n\n[webui]\nport = 23001\n\n[maim_message]\nws_server_port = 23002\n',
    model_config: '[inner]\nversion = "1.17.22"\n',
    adapter_config: '[plugin]\nconfig_version = "0.1.0"\nenabled = false\n',
};

export const YUNZAI_TEXT: Record<string, string> = {
    bot: '# 日志等级 trace,debug,info,warn,fatal,mark,error,off\nlog_level: info\n# 单条日志长度\nlog_length: 10000\n# 对象日志格式\nlog_object: true\n# 日志ID对齐\nlog_align: "  TRSSYz  "\n# 插件加载超时\nplugin_load_timeout: 60\n# 监听文件变化\nfile_watch: true\n\n# 自动更新时间\nupdate_time: 1440\n# 自动重启时间\nrestart_time: 0\n# 定时更新cron表达式\nupdate_cron:\n# 定时重启cron表达式\nrestart_cron:\n# 定时关机cron表达式\nstop_cron:\n# 定时开机cron表达式\nstart_cron:\n\n# 缓存群成员列表\ncache_group_member: true\n# 上线推送通知的冷却时间\nonline_msg_exp: 1440\n# 文件保存时间\nfile_to_url_time: 1\n# 文件访问次数\nfile_to_url_times:\n# 消息类型统计\nmsg_type_count: false\n# 以/开头转为#\n/→#: true\n\n# chromium其他路径\nchromium_path:\n# puppeteer接口地址\npuppeteer_ws:\n# puppeteer截图超时时间\npuppeteer_timeout:\n\n# 米游社接口代理地址，国际服用\nproxyAddress:\n',
    other: '# 主人QQ号\nmasterQQ:\n# Bot号:主人号\nmaster:\n',
    group: 'default:\n  groupCD: 500\n  singleCD: 2000\n  onlyReplyAt: 0\n  botAlias:\n    - 云崽\n    - 云宝\n',
    server: '# 服务器地址\nurl: http://localhost:2536\n# 服务器端口\nport: 2536\n# 服务器缺省跳转地址\nredirect: https://git.trss.me/Yunzai\n# 服务器鉴权\nauth:\n# Authorization: Bearer <access_token>\nhttps:\n# url: https://localhost:2537\n# port: 2537\n# key: config/localhost.key\n# cert: config/localhost.crt\n',
    redis: 'path: redis-server\nhost: 127.0.0.1\nport: 2537\nusername:\npassword:\ndb: 0\n',
    renderer: '# 渲染后端，留空自动\nname:\n',
    db: '# Sequelize 配置\n# https://sequelize.org/docs/v6/getting-started\n\n# mysql, postgres, sqlite, db2, mariadb, mssql\ndialect: sqlite\n\n# SQLite 文件地址\nstorage: data/db/data.db\n\n# 日志\nlogging: false\n',
    milky: '# Milky 协议配置\n\n# 是否启用\nenable: false\n\n# Milky 服务器地址\nhost: 127.0.0.1\n# Milky 服务器端口\nport: 3000\n# URL 前缀\nprefix: ""\n# 鉴权 Token\naccess_token: ""\n\n# 事件接收方式: ws (WebSocket) 或 webhook\nconnection: ws\n\n# WebHook 设置 (仅当 connection 为 webhook 时有效)\nwebhook:\n  # WebHook 路径\n  path: /milky\n\n# WebSocket 设置 (仅当 connection 为 ws 时有效)\nws:\n  # 心跳间隔 (单位：秒)\n  heartbeat: 30\n  # 断线重连间隔 (单位：秒)\n  reconnect_interval: 10\n\n# HTTP 请求超时 (单位：秒)\nhttp_timeout: 15\n',
    satori: '# Satori 协议配置\n\n# 是否启用 Satori 适配器\nenable: false\n\n# HTTP API 终结点\nhttp_endpoint: http://127.0.0.1:5140/satori/v1\n# liteloader onebot： http://127.0.0.1:5600/v1\n# server-satori： http://127.0.0.1:5140/satori/v1\n\n# WebSocket 事件终结点\nws_endpoint: ws://127.0.0.1:5140/satori/v1/events\n# liteloader onebot： ws://127.0.0.1:5600/v1/events\n# server-satori： ws://127.0.0.1:5140/satori/v1/events\n\n# API 访问令牌\ntoken: ""\n\n# 平台名称\nplatform: satori\n\n# 超时时间 (毫秒)\ntimeout: 60000\n\n# 心跳间隔 (毫秒)\nheartbeat_interval: 10000\n',
};

export const ASTRBOT_TEXT: Record<string, string> = {
    cmd_config: `${JSON.stringify(
        {
            dashboard: { port: 6185 },
            platform: [
                {
                    id: 'ncd-app:ab12cd34',
                    type: 'aiocqhttp',
                    enable: true,
                    ws_reverse_host: '0.0.0.0',
                    ws_reverse_port: 6199,
                    ws_reverse_token: '',
                },
            ],
        },
        null,
        2,
    )}\n`,
};

export const NEOBOT_TEXT: Record<string, string> = {
    adapter: `[bot]
qq = 10001

[adapter]
mode = "onebot"
reverse_ws_host = "127.0.0.1"
reverse_ws_port = 8080
reverse_ws_access_token = "ncd-mock-token"
`,
    dashboard: `host = "127.0.0.1"
port = 9981
`,
};

export const NONEBOT2_TEXT: Record<string, string> = {
    env: 'ENVIRONMENT=prod\n',
    env_prod: 'DRIVER=~fastapi+~websockets\nHOST=127.0.0.1\nPORT=8080\nONEBOT_ACCESS_TOKEN=mock\n',
    pyproject:
        '[project]\nname = "nonebot2-instance"\nversion = "0.1.0"\nrequires-python = ">=3.10,<3.14"\ndependencies = [\n  "nonebot2[fastapi,websockets]>=2.3",\n  "nonebot-adapter-onebot>=2.4",\n]\n\n[tool.nonebot]\nadapters = [{ name = "OneBot V11", module_name = "nonebot.adapters.onebot.v11" }]\nplugins = []\n',
};
