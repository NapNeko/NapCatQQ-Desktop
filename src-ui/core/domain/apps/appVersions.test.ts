import { describe, expect, it } from 'vitest';
import {
    compareAppVersion,
    hasAppUpdate,
    isPrerelease,
    latestStable,
    parsePep440,
} from './appVersions';

describe('parsePep440', () => {
    it('接受上游真实用过的拼写', () => {
        for (const raw of [
            '1.2.0',
            '1.2.1a1',
            '1.0.0a7',
            '2.0.0rc1',
            '1.0.0.post1',
            '1.0.0.dev1',
            '1.0.0-alpha.23',
            '1.2',
            '1!1.0.0',
        ]) {
            expect(parsePep440(raw), raw).not.toBeNull();
        }
        expect(parsePep440('')).toBeNull();
        expect(parsePep440('weird-version')).toBeNull();
    });

    it('拆出 release 与预发布/后发布/dev', () => {
        expect(parsePep440('1.2.1a1')).toMatchObject({
            epoch: 0,
            release: [1, 2, 1],
            pre: { kind: 'a', num: 1 },
            post: null,
            dev: null,
        });
        expect(parsePep440('1.0.0.post2')?.post).toBe(2);
        expect(parsePep440('1.0.0.dev3')?.dev).toBe(3);
        expect(parsePep440('1!2.0')?.epoch).toBe(1);
    });

    it('rc 不会被 c 抢走、preview 不会被 pre 抢走', () => {
        expect(parsePep440('1.0.0rc2')?.pre).toEqual({ kind: 'rc', num: 2 });
        expect(parsePep440('1.0.0preview1')?.pre).toEqual({ kind: 'rc', num: 1 });
        expect(parsePep440('1.0.0beta')?.pre).toEqual({ kind: 'b', num: 0 });
    });
});

describe('compareAppVersion 极性（> 0 = remote 更新）', () => {
    it('同一 release 段内预发布低于正式版', () => {
        expect(compareAppVersion('1.0.0a7', '1.0.0')).toBeGreaterThan(0);
        expect(compareAppVersion('1.0.0', '1.0.0a7')).toBeLessThan(0);
    });

    it('release 段更高时，预发布也高于对方的正式版', () => {
        expect(compareAppVersion('1.2.0', '1.2.1a1')).toBeGreaterThan(0);
        expect(compareAppVersion('1.2.1a1', '1.2.0')).toBeLessThan(0);
    });

    it('关键回归：1.2.1 要能看出 1.2.2 是更新', () => {
        expect(compareAppVersion('1.2.1', '1.2.2')).toBeGreaterThan(0);
        expect(compareAppVersion('1.2.2', '1.2.1')).toBeLessThan(0);
        expect(compareAppVersion('1.2.2', '1.2.2')).toBe(0);
    });

    it('dev < a < b < rc < 正式 < post', () => {
        expect(compareAppVersion('1.0.0.dev1', '1.0.0a1')).toBeGreaterThan(0);
        expect(compareAppVersion('1.0.0a1', '1.0.0b1')).toBeGreaterThan(0);
        expect(compareAppVersion('1.0.0b1', '1.0.0rc1')).toBeGreaterThan(0);
        expect(compareAppVersion('1.0.0rc1', '1.0.0')).toBeGreaterThan(0);
        expect(compareAppVersion('1.0.0', '1.0.0.post1')).toBeGreaterThan(0);
    });

    it('短的补 0、按数字比不按字符串', () => {
        expect(compareAppVersion('1.2', '1.2.0')).toBe(0);
        expect(compareAppVersion('1.9.0', '1.10.0')).toBeGreaterThan(0);
    });

    it('epoch 优先', () => {
        expect(compareAppVersion('2.0.0', '1!1.0.0')).toBeGreaterThan(0);
    });

    it('解析不了就不下结论（返回 0），不猜方向', () => {
        expect(compareAppVersion('garbage', '1.0.0')).toBe(0);
        expect(compareAppVersion('1.0.0', 'garbage')).toBe(0);
        expect(compareAppVersion('', '1.0.0')).toBe(0);
    });
});

describe('hasAppUpdate', () => {
    it('装了旧版才提示', () => {
        expect(hasAppUpdate('1.2.1', '1.2.2')).toBe(true);
        expect(hasAppUpdate('1.2.2', '1.2.2')).toBe(false);
        expect(hasAppUpdate('1.2.3', '1.2.2')).toBe(false);
    });

    it('没有已装版本或没有上游版本都不提示', () => {
        expect(hasAppUpdate(undefined, '1.2.2')).toBe(false);
        expect(hasAppUpdate('1.2.1', null)).toBe(false);
        expect(hasAppUpdate('', '1.2.2')).toBe(false);
    });
});

describe('isPrerelease / latestStable', () => {
    it('dev 也算预发布', () => {
        expect(isPrerelease(parsePep440('1.0.0')!)).toBe(false);
        expect(isPrerelease(parsePep440('1.0.0a1')!)).toBe(true);
        expect(isPrerelease(parsePep440('1.0.0.dev1')!)).toBe(true);
        expect(isPrerelease(parsePep440('1.0.0.post1')!)).toBe(false);
    });

    it('从降序列表里挑第一个正式版', () => {
        expect(latestStable(['1.2.2', '1.2.1', '1.0.0a25'])).toBe('1.2.2');
        expect(latestStable(['1.2.2a1', '1.0.0a25'])).toBeNull();
        expect(latestStable([])).toBeNull();
        expect(latestStable(['garbage', '1.0.0'])).toBe('1.0.0');
    });
});
