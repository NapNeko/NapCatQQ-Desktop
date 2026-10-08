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
        '[inner]\nversion = "8.14.40"\n\n[webui]\nport = 23001\n\n[maim_message]\nws_server_port = 23002\n',
    model_config: '[inner]\nversion = "1.17.9"\n',
    adapter_config: '[plugin]\nconfig_version = "0.1.0"\nenabled = false\n',
};

export const YUNZAI_TEXT: Record<string, string> = {
    bot: '# 日志等级\nlog_level: info\n# 渲染用的浏览器，留空用装时下载的\nchromium_path:\n',
    other: '# 主人QQ号\nmasterQQ:\n# Bot号:主人号\nmaster:\n',
    group: 'default:\n  groupCD: 500\n  singleCD: 2000\n  onlyReplyAt: 0\n  botAlias:\n    - 云崽\n    - 云宝\n',
    server: 'url: http://localhost:2536\nport: 2536\nredirect: https://git.trss.me/Yunzai\nauth:\n',
    redis: 'path: redis-server\nhost: 127.0.0.1\nport: 2537\nusername:\npassword:\ndb: 0\n',
    renderer: '# 渲染后端，留空自动\nname:\n',
    db: 'dialect: sqlite\nstorage: data/db/data.db\nlogging: false\n',
    milky: '# Milky 协议端，Desktop 不用\n',
    satori: '# Satori 协议端，Desktop 不用\n',
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
