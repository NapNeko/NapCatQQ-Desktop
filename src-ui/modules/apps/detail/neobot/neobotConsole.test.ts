import { describe, expect, it } from 'vitest';
import {
    CONSOLE_STATE_LABEL,
    NEOBOT_CONSOLE_FEATURES,
    consoleFeatureStats,
} from './neobotConsole';

describe('NeoBot 控制台能力地图', () => {
    it('每一项都有名字、说明、状态与端点', () => {
        for (const g of NEOBOT_CONSOLE_FEATURES) {
            expect(g.title, g.id).not.toBe('');
            expect(g.items.length, g.id).toBeGreaterThan(0);
            for (const item of g.items) {
                expect(item.name).not.toBe('');
                expect(item.desc).not.toBe('');
                expect(CONSOLE_STATE_LABEL[item.state], item.name).toBeTruthy();
                expect(item.endpoints.length, item.name).toBeGreaterThan(0);
            }
        }
    });

    it('端点写成 /api 前缀，别把面板口或主机名混进来', () => {
        for (const g of NEOBOT_CONSOLE_FEATURES) {
            for (const item of g.items) {
                for (const ep of item.endpoints) {
                    expect(ep.startsWith('/'), item.name + ' ' + ep).toBe(true);
                    expect(ep.includes('://'), item.name + ' ' + ep).toBe(false);
                    expect(ep.includes(':'), item.name + ' ' + ep).toBe(false);
                }
            }
        }
    });

    it('名字不重复（地图是按名字做 key 的）', () => {
        const names = NEOBOT_CONSOLE_FEATURES.flatMap((g) => g.items.map((i) => i.name));
        expect(new Set(names).size).toBe(names.length);
    });

    it('汇总数与明细一致', () => {
        const { total, byState } = consoleFeatureStats();
        const items = NEOBOT_CONSOLE_FEATURES.flatMap((g) => g.items);
        expect(total).toBe(items.length);
        expect(byState.desktop + byState.partial + byState.consoleOnly).toBe(total);
        expect(byState.consoleOnly).toBe(items.filter((i) => i.state === 'consoleOnly').length);
    });

    it('日志与原始文件这类外壳已提供的，不该标成「仅控制台」', () => {
        const logs = NEOBOT_CONSOLE_FEATURES.flatMap((g) => g.items).find((i) => i.name === '日志');
        expect(logs?.state).toBe('desktop');
    });

    it('汇总允许传入自定义分组（页面按默认值渲染，测试要能换数据）', () => {
        const stats = consoleFeatureStats([
            { id: 'x', title: 'X', items: [{ name: 'a', desc: 'd', state: 'partial', endpoints: ['/api/a'] }] },
        ]);
        expect(stats.total).toBe(1);
        expect(stats.byState.partial).toBe(1);
    });
});
