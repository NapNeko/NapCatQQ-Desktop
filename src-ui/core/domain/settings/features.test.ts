import { describe, expect, it } from 'vitest';
import {
    DEFAULT_FEATURES,
    EMPTY_FEATURE_USAGE,
    FEATURE_DEFS,
    appFrameworkOffBlock,
    featureOffBlock,
    featureOffWarning,
    featuresEqual,
    isAppFrameworkVisible,
    isComponentHiddenByFeatures,
    normalizeFeatures,
    setAppFrameworkVisible,
} from './features';

describe('normalizeFeatures', () => {
    it('缺字段、空值一律当开着', () => {
        expect(normalizeFeatures(undefined)).toEqual(DEFAULT_FEATURES);
        expect(normalizeFeatures(null)).toEqual(DEFAULT_FEATURES);
        expect(normalizeFeatures({})).toEqual(DEFAULT_FEATURES);
    });

    it('只有明确的 false 才算关', () => {
        expect(normalizeFeatures({ terminal: false, apps: 'no', dockerPage: 0 })).toMatchObject({
            apps: true,
            dockerPage: true,
            terminal: false,
        });
    });

    it('两个协议端都关了就开回 NapCat', () => {
        expect(normalizeFeatures({ napcat: false, snowluma: false })).toMatchObject({
            napcat: true,
            snowluma: false,
        });
    });

    it('聊天独立于调试台开关，旧设置默认开启', () => {
        expect(normalizeFeatures({ apiDebug: false })).toMatchObject({
            apiDebug: false,
            chat: true,
        });
        expect(normalizeFeatures({ chat: false })).toMatchObject({
            apiDebug: true,
            chat: false,
        });
    });

    it('框架名单去重、丢掉空值和非字符串', () => {
        expect(
            normalizeFeatures({ hiddenAppFrameworks: ['karin', 'karin', '', 3, 'maibot'] })
                .hiddenAppFrameworks,
        ).toEqual(['karin', 'maibot']);
    });
});

describe('featuresEqual', () => {
    it('逐项比较，框架名单不看顺序', () => {
        expect(featuresEqual(DEFAULT_FEATURES, { ...DEFAULT_FEATURES })).toBe(true);
        expect(featuresEqual(DEFAULT_FEATURES, { ...DEFAULT_FEATURES, apps: false })).toBe(false);
        expect(featuresEqual(DEFAULT_FEATURES, { ...DEFAULT_FEATURES, chat: false })).toBe(false);
        const a = { ...DEFAULT_FEATURES, hiddenAppFrameworks: ['karin', 'maibot'] };
        const b = { ...DEFAULT_FEATURES, hiddenAppFrameworks: ['maibot', 'karin'] };
        expect(featuresEqual(a, b)).toBe(true);
        expect(featuresEqual(a, { ...DEFAULT_FEATURES, hiddenAppFrameworks: ['karin'] })).toBe(
            false,
        );
    });
});

describe('应用端框架', () => {
    it('总开关关了一个都不显示；单独藏的只藏它', () => {
        const hidden = setAppFrameworkVisible(DEFAULT_FEATURES, 'karin', false);
        expect(isAppFrameworkVisible(hidden, 'karin')).toBe(false);
        expect(isAppFrameworkVisible(hidden, 'maibot')).toBe(true);
        expect(isAppFrameworkVisible({ ...DEFAULT_FEATURES, apps: false }, 'maibot')).toBe(false);
        expect(setAppFrameworkVisible(hidden, 'karin', true).hiddenAppFrameworks).toEqual([]);
    });

    it('有实例的框架不让藏', () => {
        const usage = { ...EMPTY_FEATURE_USAGE, instancesByFramework: { karin: 2 } };
        expect(appFrameworkOffBlock('karin', usage)).toContain('2 个实例');
        expect(appFrameworkOffBlock('maibot', usage)).toBeNull();
    });
});

describe('isComponentHiddenByFeatures', () => {
    it('协议端和 ncd-watch 跟着开关，其它组件不受影响', () => {
        const f = { ...DEFAULT_FEATURES, snowluma: false, ncdWatch: false };
        expect(isComponentHiddenByFeatures(f, 'snowluma')).toBe(true);
        expect(isComponentHiddenByFeatures(f, 'ncd_watch')).toBe(true);
        expect(isComponentHiddenByFeatures(f, 'napcat')).toBe(false);
        expect(isComponentHiddenByFeatures(f, 'qq')).toBe(false);
    });
});

describe('featureOffBlock', () => {
    it('协议端：有 Bot 在用、或另一个已经关了，都不让关', () => {
        const usage = { ...EMPTY_FEATURE_USAGE, botsByBackend: { napcat: 0, snowluma: 3 } };
        expect(featureOffBlock('snowluma', DEFAULT_FEATURES, usage)).toContain('3 个 Bot');
        expect(featureOffBlock('napcat', DEFAULT_FEATURES, usage)).toBeNull();
        expect(
            featureOffBlock(
                'napcat',
                { ...DEFAULT_FEATURES, snowluma: false },
                EMPTY_FEATURE_USAGE,
            ),
        ).toBe('两个协议端至少留一个');
    });

    it('应用端有实例在跑、远端装着 ncd-watch 时不让关', () => {
        expect(
            featureOffBlock('apps', DEFAULT_FEATURES, {
                ...EMPTY_FEATURE_USAGE,
                activeAppInstances: 1,
            }),
        ).toContain('1 个实例');
        expect(
            featureOffBlock('ncdWatch', DEFAULT_FEATURES, {
                ...EMPTY_FEATURE_USAGE,
                hostsWithNcdWatch: 2,
            }),
        ).toContain('2 台远端主机');
        expect(featureOffBlock('terminal', DEFAULT_FEATURES, EMPTY_FEATURE_USAGE)).toBeNull();
    });

    it('关终端只提醒会关掉开着的', () => {
        expect(
            featureOffWarning('terminal', { ...EMPTY_FEATURE_USAGE, openTerminals: 2 }),
        ).toContain('2 个终端');
        expect(featureOffWarning('terminal', EMPTY_FEATURE_USAGE)).toBeNull();
    });
});

describe('FEATURE_DEFS', () => {
    it('每个布尔开关都有一条说明，且不重复', () => {
        const keys = FEATURE_DEFS.map((d) => d.key);
        expect(new Set(keys).size).toBe(keys.length);
        const boolKeys = Object.keys(DEFAULT_FEATURES).filter((k) => k !== 'hiddenAppFrameworks');
        expect([...keys].sort()).toEqual(boolKeys.sort());
    });
});
