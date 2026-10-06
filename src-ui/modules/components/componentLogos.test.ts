import { describe, expect, it } from 'vitest';
import { componentLogo, componentLogoIcon } from './componentLogos';

describe('componentLogo', () => {
    it.each(['napcat', 'snowluma', 'nodejs', 'uv', 'git', 'redis', 'karin', 'astrbot', 'neobot'])(
        '%s 有头像',
        (id) => {
            expect(componentLogo(id)).toEqual(expect.any(String));
            expect(componentLogo(id)!.length).toBeGreaterThan(0);
        },
    );

    it('缺图或未知 id 返回 undefined，卡片不留占位', () => {
        expect(componentLogo('qq')).toBeUndefined();
        expect(componentLogo('not-a-component')).toBeUndefined();
        expect(componentLogo('')).toBeUndefined();
        expect(componentLogoIcon('qq')).toBeUndefined();
    });

    it('有图时给出 icon 节点', () => {
        expect(componentLogoIcon('neobot')).toBeTruthy();
    });
});
