import { describe, expect, it } from 'vitest';
import {
    NEOBOT_MIN_VERSION,
    meetsNeoBotVersion,
    versionRequirementText,
} from './neobotCapabilities';

describe('meetsNeoBotVersion', () => {
    it('比最低版本新或相等就算够', () => {
        expect(meetsNeoBotVersion('1.2.4a1', 'deployApi')).toBe(true);
        expect(meetsNeoBotVersion('1.2.4', 'deployApi')).toBe(true);
        expect(meetsNeoBotVersion('1.3.0', 'deployApi')).toBe(true);
    });

    it('实测的那种情况：1.2.3 的实例够不到快捷部署与面板登录', () => {
        expect(meetsNeoBotVersion('1.2.3', 'deployApi')).toBe(false);
        expect(meetsNeoBotVersion('1.2.3', 'panelLogin')).toBe(false);
        // 但优雅关闭是 1.2.3 就有的
        expect(meetsNeoBotVersion('1.2.3', 'gracefulStop')).toBe(true);
    });

    it('预发布序：1.2.4a1 低于 1.2.4，但高于 1.2.3', () => {
        expect(meetsNeoBotVersion('1.2.4a1', 'panelLogin')).toBe(true);
        expect(meetsNeoBotVersion('1.2.4b1', 'panelLogin')).toBe(true);
        expect(meetsNeoBotVersion('1.2.3', 'panelLogin')).toBe(false);
    });

    it('版本未知时不拦：宁可让请求去打，也别把能用的页面挡掉', () => {
        expect(meetsNeoBotVersion(null, 'deployApi')).toBe(true);
        expect(meetsNeoBotVersion(undefined, 'deployApi')).toBe(true);
        expect(meetsNeoBotVersion('   ', 'deployApi')).toBe(true);
    });

    it('解析不了的版本号也不拦（compareAppVersion 返回 0）', () => {
        expect(meetsNeoBotVersion('not-a-version', 'deployApi')).toBe(true);
    });
});

describe('versionRequirementText', () => {
    it('把「要哪版、现在哪版」都说出来', () => {
        expect(versionRequirementText('1.2.3', 'deployApi')).toBe(
            '需要 NeoBot ' + NEOBOT_MIN_VERSION.deployApi + ' 及以上，当前 1.2.3',
        );
    });

    it('版本未知时写「未知」，不留空', () => {
        expect(versionRequirementText(null, 'panelLogin')).toContain('当前 未知');
    });
});
