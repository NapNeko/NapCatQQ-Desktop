import { describe, expect, it } from 'vitest';
import { frameworkLogo } from './frameworkLogos';

describe('frameworkLogo', () => {
    it('NeoBot 有头图（它的 key 必须与 manifest.id 的 serde 名一致）', () => {
        expect(frameworkLogo('neobot')).toBeTruthy();
    });

    it('没配头图的框架返回 undefined，调用方不渲染占位', () => {
        expect(frameworkLogo('astrbot')).toBeUndefined();
        expect(frameworkLogo('not-a-framework')).toBeUndefined();
        expect(frameworkLogo('')).toBeUndefined();
    });

    it('返回的是可用的图片地址（构建后是带 hash 的资源路径）', () => {
        const logo = frameworkLogo('neobot');
        expect(typeof logo).toBe('string');
        expect(logo!.length).toBeGreaterThan(0);
    });
});
