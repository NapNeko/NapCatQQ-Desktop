import { describe, expect, it } from 'vitest';
import {
    newYunzaiGroupOverride,
    validateYunzaiConfig,
    yunzaiDefaultConfig,
    yunzaiLinkInputsChanged,
    yunzaiNeedsMaster,
    yunzaiOverrideFieldCount,
    yunzaiOverrideScope,
    yunzaiPluginToggleable,
    yunzaiRestartInputsChanged,
} from './yunzaiConfig';

describe('yunzaiConfig', () => {
    it('默认配置能直接保存', () => {
        expect(validateYunzaiConfig(yunzaiDefaultConfig(24100))).toEqual([]);
    });

    it('校验路径和后端一致', () => {
        const cfg = yunzaiDefaultConfig(24100);
        cfg.bot.log_level = 'loud';
        cfg.redis.port = 24100;
        cfg.other.master = ['123'];
        cfg.other.master_qq = ['1 2'];
        cfg.group.overrides = [
            newYunzaiGroupOverride('default'),
            newYunzaiGroupOverride('555'),
            newYunzaiGroupOverride('555'),
        ];
        const paths = validateYunzaiConfig(cfg).map((i) => i.path);
        expect(paths).toEqual(
            expect.arrayContaining([
                'bot/log_level',
                'redis/port',
                'other/master/0',
                'other/master_qq/0',
                'group/overrides/0/key',
                'group/overrides/2/key',
            ]),
        );
        expect(paths).not.toContain('group/overrides/1/key');
    });

    it('Redis 连外部主机时端口可以和云崽相同', () => {
        const cfg = yunzaiDefaultConfig(24100);
        cfg.redis.host = '10.0.0.2';
        cfg.redis.port = 24100;
        expect(validateYunzaiConfig(cfg)).toEqual([]);
    });

    it('对接只看端口和 token；重启看启动时读的那些', () => {
        const a = yunzaiDefaultConfig(24100);
        const b = structuredClone(a);
        b.group.default.group_cd = 0;
        expect(yunzaiLinkInputsChanged(a, b)).toBe(false);
        expect(yunzaiRestartInputsChanged(a, b)).toBe(false);
        b.server.access_token = 'x';
        expect(yunzaiLinkInputsChanged(a, b)).toBe(true);
        const c = structuredClone(a);
        c.bot.update_cron = ['0 4 * * *'];
        expect(yunzaiRestartInputsChanged(a, c)).toBe(true);
    });

    it('主人和单独设置的说明', () => {
        const cfg = yunzaiDefaultConfig(24100);
        expect(yunzaiNeedsMaster(cfg)).toBe(true);
        cfg.other.master_qq = ['10001'];
        expect(yunzaiNeedsMaster(cfg)).toBe(false);
        expect(yunzaiOverrideScope('123456')).toBe('群 123456');
        expect(yunzaiOverrideScope('114514:default')).toBe('Bot 114514 在所有群');
        expect(yunzaiOverrideScope('114514:123456')).toBe('Bot 114514 在群 123456');
        expect(yunzaiOverrideFieldCount({ key: '1', group_cd: 0, bot_alias: [] })).toBe(2);
        expect(yunzaiOverrideFieldCount({ key: '1' })).toBe(0);
    });

    it('只有单 JS 能单独启停', () => {
        expect(yunzaiPluginToggleable('chuo.js')).toBe(true);
        expect(yunzaiPluginToggleable('miao-plugin')).toBe(false);
    });
    it('新增协议字段的错误能定位，修改后会提示重启', () => {
        const a = yunzaiDefaultConfig(24100);
        const b = structuredClone(a);
        b.server.https = { port: 24100, key: 'server.key', cert: 'server.crt', url: '' };
        b.milky.enable = true;
        b.milky.connection = 'other';
        b.satori.enable = true;
        b.satori.ws_endpoint = 'http://wrong';
        const paths = validateYunzaiConfig(b).map((i) => i.path);
        expect(paths).toEqual(expect.arrayContaining([
            'server/https/port', 'milky/connection', 'satori/ws_endpoint',
        ]));
        expect(yunzaiRestartInputsChanged(a, b)).toBe(true);
        expect(yunzaiLinkInputsChanged(a, b)).toBe(false);
        const c = structuredClone(a);
        c.db.logging = true;
        expect(yunzaiRestartInputsChanged(a, c)).toBe(true);
    });

    it('文件访问次数的 0 和不限制是不同配置', () => {
        const cfg = yunzaiDefaultConfig(24100);
        expect(cfg.bot.file_to_url_times).toBeUndefined();
        cfg.bot.file_to_url_times = 0;
        expect(validateYunzaiConfig(cfg)).toEqual([]);
        cfg.bot.file_to_url_times = -1;
        expect(validateYunzaiConfig(cfg).map((i) => i.path)).toContain('bot/file_to_url_times');
    });

    it('Satori 的空地址和零时间沿用上游默认值', () => {
        const cfg = yunzaiDefaultConfig(24100);
        Object.assign(cfg.satori, {
            enable: true, http_endpoint: '', ws_endpoint: '', timeout: 0, heartbeat_interval: 0,
        });
        expect(validateYunzaiConfig(cfg)).toEqual([]);
    });
});
