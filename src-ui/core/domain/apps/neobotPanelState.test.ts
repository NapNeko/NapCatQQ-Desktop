import { describe, expect, it } from 'vitest';
import { panelPageGate, panelQueryState, type PanelState } from './neobotPanelState';

const ok = (data: unknown = {}): PanelState<unknown> => ({ kind: 'ok', data });

describe('panelPageGate', () => {
    it('全是未授权：整页只出一条密码提示（就是实测里重复六遍的那种页面）', () => {
        const states: PanelState<unknown>[] = Array.from({ length: 6 }, () => ({
            kind: 'unauthorized',
            message: '面板要求登录',
        }));
        expect(panelPageGate(states)).toEqual({
            kind: 'unauthorized',
            message: '面板要求登录',
            blocked: 6,
            total: 6,
        });
    });

    it('只要有一条读到了，就不挡整页：剩下的故障归各自小节', () => {
        expect(panelPageGate([ok(), { kind: 'unauthorized', message: '' }])).toBeNull();
    });

    it('全部还在读时给 loading，别先闪故障卡', () => {
        expect(panelPageGate([undefined, undefined])).toEqual({
            kind: 'loading',
            message: '',
            blocked: 0,
            total: 2,
        });
    });

    it('读了一部分、都失败：给出故障，不当作还在读', () => {
        expect(
            panelPageGate([undefined, { kind: 'unreachable', message: 'ECONNREFUSED' }]),
        ).toEqual({ kind: 'unreachable', message: 'ECONNREFUSED', blocked: 1, total: 2 });
    });

    it('多种故障混在一起时按「不支持 > 没凭据 > 打不通」取一条', () => {
        expect(
            panelPageGate([
                { kind: 'unreachable', message: '' },
                { kind: 'unauthorized', message: '' },
            ])?.kind,
        ).toBe('unauthorized');
        expect(
            panelPageGate([{ kind: 'unsupported' }, { kind: 'unauthorized', message: '' }])?.kind,
        ).toBe('unsupported');
    });

    it('全是不带整页含义的失败（404 / 形状不认识）时归为 unavailable，并带上第一句人话', () => {
        expect(
            panelPageGate([
                { kind: 'notFound', message: '' },
                { kind: 'malformed' },
                { kind: 'failed', message: '面板拒绝了这次请求' },
            ]),
        ).toEqual({
            kind: 'unavailable',
            message: '面板拒绝了这次请求',
            blocked: 3,
            total: 3,
        });
    });

    it('空列表不算故障（用于还没发请求的页面）', () => {
        expect(panelPageGate([])).toBeNull();
    });
});

describe('panelQueryState', () => {
    it('首次请求异常可转为整页可重试错误', () => {
        const state = panelQueryState({
            data: undefined,
            isError: true,
            error: new Error('连接中断'),
        });
        expect(panelPageGate([state])?.kind).toBe('unavailable');
        expect(state).toEqual({ kind: 'failed', message: '连接中断' });
    });

    it('后台刷新失败保留已读取的数据', () => {
        const data = ok({ cpu_percent: 20 });
        expect(panelQueryState({ data, isError: true, error: new Error('断线') })).toBe(data);
    });
});
