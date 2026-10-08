import { describe, expect, it } from 'vitest';
import type { AppConfigWriteResult, KarinInstanceConfig } from '../../ipc/types';
import {
    KARIN_CONFIG_FORM,
    KARIN_GROUP_RULE_KEYS,
    KARIN_LOG_LEVELS,
    KARIN_NODE_ENVS,
    KARIN_PRIVATE_RULE_KEYS,
    KARIN_RULE_MODES,
    KARIN_RUNTIMES,
    karinDefaultConfig,
    karinEnvToDotenv,
    karinLinkInputsChanged,
    newGroupRule,
    newOneBotHttpServer,
    newOneBotWsClient,
    newPrivateRule,
    newRenderHttpServer,
    newRenderWsClient,
    validateKarinConfig,
} from './karinConfig';

const paths = (cfg: KarinInstanceConfig) => validateKarinConfig(cfg).map((i) => i.path);

describe('karin 工厂函数', () => {
    it('群规则带 userCD / member_* 默认值，可用 key 覆盖', () => {
        const r = newGroupRule();
        expect(r).toEqual({
            key: 'Bot:selfId:groupId',
            inherit: true,
            cd: 0,
            userCD: 0,
            mode: 0,
            alias: [],
            enable: [],
            disable: [],
            member_enable: [],
            member_disable: [],
        });
        expect(newGroupRule('default').key).toBe('default');
    });

    it('私聊规则没有 userCD / member_*', () => {
        expect(newPrivateRule('default')).toEqual({
            key: 'default',
            inherit: true,
            cd: 0,
            mode: 0,
            alias: [],
            enable: [],
            disable: [],
        });
    });

    it('onebot / render 四个子块给出上游默认端点', () => {
        expect(newOneBotWsClient()).toEqual({
            enable: false,
            url: 'ws://127.0.0.1:7778',
            token: '',
        });
        expect(newOneBotHttpServer()).toEqual({
            enable: false,
            self_id: 'default',
            url: 'http://127.0.0.1:6099',
            token: '',
            api_token: '',
            post_token: '',
        });
        expect(newRenderWsClient()).toEqual({
            enable: false,
            url: 'ws://127.0.0.1:7005',
            token: '123456',
            isSnapka: false,
            reconnectTime: 5000,
            heartbeatTime: 30000,
        });
        expect(newRenderHttpServer()).toEqual({
            enable: false,
            url: 'http://127.0.0.1:7005',
            token: '123456',
            isSnapka: false,
        });
    });

    it('默认配置以传入端口起步，规则表覆盖全部白名单键', () => {
        const cfg = karinDefaultConfig(8123);
        expect(cfg.env.http_port).toBe(8123);
        expect(karinDefaultConfig().env.http_port).toBe(7777);
        expect(cfg.groups.map((r) => r.key)).toEqual([...KARIN_GROUP_RULE_KEYS]);
        expect(cfg.privates.map((r) => r.key)).toEqual([...KARIN_PRIVATE_RULE_KEYS]);
        expect(cfg.redis.url).toBe('redis://127.0.0.1:6379');
        expect(cfg.adapter.onebot.ws_server).toEqual({ enable: true, timeout: 120 });
    });

    it('每次调用返回互不共享的新对象', () => {
        const a = karinDefaultConfig();
        a.env.http_port = 9999;
        expect(karinDefaultConfig().env.http_port).toBe(7777);
    });
});

describe('validateKarinConfig', () => {
    it('上游默认配置本身合法', () => {
        expect(validateKarinConfig(karinDefaultConfig())).toEqual([]);
    });

    it('端口只认 1-65535 的整数', () => {
        const bad = (port: number) => {
            const cfg = karinDefaultConfig();
            cfg.env.http_port = port;
            return paths(cfg);
        };
        expect(bad(0)).toContain('env/http_port');
        expect(bad(65536)).toContain('env/http_port');
        expect(bad(80.5)).toContain('env/http_port');
        expect(bad(1)).toEqual([]);
        expect(bad(65535)).toEqual([]);
    });

    it('log_level / runtime / node_env 限定枚举', () => {
        const cfg = karinDefaultConfig();
        cfg.env.log_level = 'verbose';
        cfg.env.runtime = 'bun';
        cfg.env.node_env = 'staging';
        expect(paths(cfg)).toEqual(
            expect.arrayContaining(['env/log_level', 'env/runtime', 'env/node_env']),
        );
        const ok = karinDefaultConfig();
        ok.env.log_level = KARIN_LOG_LEVELS[8];
        ok.env.runtime = KARIN_RUNTIMES[1];
        ok.env.node_env = KARIN_NODE_ENVS[0];
        expect(validateKarinConfig(ok)).toEqual([]);
    });

    it('自定义 env 键拦空键 / 非法形状 / 系统键 / 重复', () => {
        const cfg = karinDefaultConfig();
        cfg.env.custom = [
            { key: '   ', value: '1', comment: '' },
            { key: '2BAD', value: '1', comment: '' },
            { key: 'HTTP_PORT', value: '1', comment: '' },
            { key: 'MY_VAR', value: '1', comment: '' },
            { key: 'MY_VAR', value: '2', comment: '' },
        ];
        expect(paths(cfg)).toEqual([
            'env/custom/0/key',
            'env/custom/1/key',
            'env/custom/2/key',
            'env/custom/4/key',
        ]);
    });

    it('onebot 超时须为正数', () => {
        const cfg = karinDefaultConfig();
        cfg.adapter.onebot.ws_server.timeout = 0;
        expect(paths(cfg)).toContain('adapter/onebot/ws_server/timeout');
    });

    it('ws 客户端认 ws:// 与 wss://，前后空格不影响', () => {
        const bad = karinDefaultConfig();
        bad.adapter.onebot.ws_client[0].url = 'http://127.0.0.1:7778';
        expect(paths(bad)).toContain('adapter/onebot/ws_client/0/url');
        const ok = karinDefaultConfig();
        ok.adapter.onebot.ws_client[0].url = ' wss://bot.example.com:7778 ';
        expect(validateKarinConfig(ok)).toEqual([]);
    });

    it('http_server 要 http(s) 端点且 self_id 非空', () => {
        const cfg = karinDefaultConfig();
        cfg.adapter.onebot.http_server[0].url = 'ws://127.0.0.1:6099';
        cfg.adapter.onebot.http_server[0].self_id = '   ';
        expect(paths(cfg)).toEqual(
            expect.arrayContaining([
                'adapter/onebot/http_server/0/url',
                'adapter/onebot/http_server/0/self_id',
            ]),
        );
    });

    it('群 / 私聊规则拦空键、重复键、mode 越界', () => {
        const cfg = karinDefaultConfig();
        cfg.groups[0].mode = 7;
        cfg.groups.push(newGroupRule('default'));
        cfg.privates[0].key = '';
        const issuePaths = paths(cfg);
        expect(issuePaths).toContain('groups/0/mode');
        expect(issuePaths).toContain(`groups/${KARIN_GROUP_RULE_KEYS.length}/key`);
        expect(issuePaths).toContain('privates/0/key');
        expect(KARIN_RULE_MODES.map((m) => m.value)).toEqual([0, 1, 2, 3, 4, 5, 6]);
        const boundary = karinDefaultConfig();
        boundary.groups.forEach((r, i) => {
            r.mode = i === 0 ? 6 : i === 1 ? 0 : r.mode;
        });
        expect(validateKarinConfig(boundary)).toEqual([]);
    });

    it('render 端点协议与 redis 前缀分开校验', () => {
        const cfg = karinDefaultConfig();
        cfg.render.ws_client[0].url = 'ws+unix:///tmp/render.sock';
        cfg.render.http_server[0].url = 'ftp://127.0.0.1:7005';
        cfg.redis.url = 'memcached://127.0.0.1:11211';
        expect(paths(cfg)).toEqual(
            expect.arrayContaining([
                'render/ws_client/0/url',
                'render/http_server/0/url',
                'redis/url',
            ]),
        );
        const ok = karinDefaultConfig();
        ok.redis.url = ' rediss://cache.internal:6380 ';
        expect(validateKarinConfig(ok)).toEqual([]);
    });
});

describe('karinLinkInputsChanged', () => {
    it('只有 http 端口或反向 WS 秘钥变了才算变', () => {
        const a = karinDefaultConfig().env;
        expect(karinLinkInputsChanged(a, { ...a })).toBe(false);
        expect(karinLinkInputsChanged(a, { ...a, http_port: 8123 })).toBe(true);
        expect(karinLinkInputsChanged(a, { ...a, ws_server_auth_key: 'k' })).toBe(true);
        expect(karinLinkInputsChanged(a, { ...a, log_level: 'debug' })).toBe(false);
    });
});

describe('karinEnvToDotenv', () => {
    it('系统键按固定顺序输出，布尔转 true/false，末尾换行', () => {
        const text = karinEnvToDotenv(karinDefaultConfig(8123).env);
        const lines = text.split('\n');
        expect(lines.at(-1)).toBe('');
        expect(lines.filter((line) => line === '')).toHaveLength(1);
        expect(lines).toContain('HTTP_ENABLE=true');
        expect(lines).toContain('HTTP_PORT=8123');
        expect(lines).toContain('TSX_WATCH=false');
        expect(lines).toContain('RUNTIME=node');
    });

    it('注释行贴在键上一行，含空格 / # / 引号的值加引号转义', () => {
        const env = karinDefaultConfig().env;
        env.http_port = 8080;
        env.comments = { HTTP_PORT: 'WebUI 与 OneBot HTTP 共用端口' };
        env.custom = [
            { key: 'MY_HOME', value: 'D:\\Program Files\\karin', comment: '自定义路径' },
            { key: 'SAY', value: '说"你好"', comment: '' },
        ];
        const text = karinEnvToDotenv(env);
        expect(text).toContain('# WebUI 与 OneBot HTTP 共用端口\nHTTP_PORT=8080');
        expect(text).toContain('\n\n# 自定义路径\nMY_HOME="D:\\Program Files\\karin"');
        expect(text).toContain('\nSAY="说\\"你好\\""\n');
    });
});

describe('KARIN_CONFIG_FORM', () => {
    it('表单规格走同一份校验，保存提示区分需不需要重启', () => {
        expect(KARIN_CONFIG_FORM.framework).toBe('karin');
        const cfg = karinDefaultConfig();
        cfg.env.http_port = 0;
        expect(KARIN_CONFIG_FORM.validate(cfg).map((i) => i.path)).toContain('env/http_port');
        const restart = { restart_required: true } as AppConfigWriteResult;
        expect(KARIN_CONFIG_FORM.saveHint(restart, true)).toContain('重启');
        expect(
            KARIN_CONFIG_FORM.saveHint({ ...restart, restart_required: false }, true),
        ).not.toBeNull();
    });
});
