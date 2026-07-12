// 读取一组 CSS custom property（design token）的当前解析值，主题变化时自动重渲染。
//
// 使用场景：组件需要把 token 颜色喂给 canvas / SVG 字符串 / 第三方库这种
// 不能用纯 CSS 表达的边界（如 qrcode 库渲染）。
//
// 实现方式：用一个临时 <span> 当探针，把 `color: var(--xxx)` 喂给它，再读
// 探针的 computed `color`。CSS color 属性的 computed value 在所有浏览器
// 一律是 `rgb(r, g, b)` / `rgba(...)` 形式，所有 var() / color-mix() 已经
// 完成替换。拿到 rgb 后再转 hex 给消费方。
//
// 之前直接用 `getComputedStyle(documentElement).getPropertyValue('--xxx')`
// 在自定义属性嵌套引用（--a: var(--b)）的场景下，部分 Chromium 实测会
// 返回未解析的字面量 `var(--b)`，把这个字符串喂给 qrcode 库会被当成无效
// 颜色，fallback 到默认黑白，于是 token 颜色完全没生效。
//
// 触发重读的事件：
//   1. <html data-theme="..."> attribute 变化（手动主题切换）
//   2. matchMedia('(prefers-color-scheme: dark)') 变化（系统主题切换）
//   3. window 上的自定义 'theme-changed' 事件（业务层未来扩展）

import { useEffect, useState } from 'react';

export type TokenMap<K extends string> = Record<K, string>;

/// 把 `rgb(r, g, b)` / `rgba(r, g, b, a)` 转 6 位 hex；非法格式返回 null。
function rgbToHex(rgb: string): string | null {
    const m = rgb.match(/^rgba?\(\s*(\d+)\s*,\s*(\d+)\s*,\s*(\d+)/);
    if (!m) return null;
    const toHex = (s: string) => Number(s).toString(16).padStart(2, '0');
    return `#${toHex(m[1])}${toHex(m[2])}${toHex(m[3])}`;
}

/// 通过临时 DOM 探针把 `var(--xxx)` 解析成 hex；fallback 必须是合法 hex。
function readTokenColor(name: string, fallback: string): string {
    if (typeof window === 'undefined' || typeof document === 'undefined') {
        return fallback;
    }
    const probe = document.createElement('span');
    // display:none 的元素 computed color 也会被解析，但有些浏览器版本对
    // display:none 的探针有优化跳过 → 用 visibility:hidden + position:absolute
    // 保证元素进入布局树但不可见、不影响交互。
    probe.style.cssText = `position:absolute;left:-9999px;visibility:hidden;color:var(${name});`;
    document.body.appendChild(probe);
    let computed = '';
    try {
        computed = getComputedStyle(probe).color;
    } finally {
        probe.remove();
    }
    return rgbToHex(computed) ?? fallback;
}

/// 批量读 token，主题变化时整组重新计算并触发组件重渲染。
///
/// 使用：
///   const { foreground, background } = useThemeTokens({
///     foreground: { name: '--qr-foreground', fallback: '#000' },
///     background: { name: '--qr-background', fallback: '#fff' },
///   });
export function useThemeTokens<K extends string>(
    spec: Record<K, { name: string; fallback: string }>,
): TokenMap<K> {
    const compute = (): TokenMap<K> => {
        const out: TokenMap<K> = {} as TokenMap<K>;
        for (const key of Object.keys(spec) as K[]) {
            out[key] = readTokenColor(spec[key].name, spec[key].fallback);
        }
        return out;
    };

    const [tokens, setTokens] = useState<TokenMap<K>>(compute);

    useEffect(() => {
        const refresh = () => setTokens(compute());

        // 1. 监听 <html data-theme="..."> 切换
        const root = document.documentElement;
        const observer = new MutationObserver((mutations) => {
            for (const m of mutations) {
                if (m.type === 'attributes' && m.attributeName === 'data-theme') {
                    refresh();
                    return;
                }
            }
        });
        observer.observe(root, { attributes: true, attributeFilter: ['data-theme'] });

        // 2. 监听系统主题
        const mq = window.matchMedia('(prefers-color-scheme: dark)');
        const onMq = () => refresh();
        // Safari 14- 用 addListener；现代浏览器走 addEventListener
        if (mq.addEventListener) {
            mq.addEventListener('change', onMq);
        } else {
            mq.addListener(onMq);
        }

        // 3. 业务层广播
        const onCustom = () => refresh();
        window.addEventListener('theme-changed', onCustom);

        return () => {
            observer.disconnect();
            if (mq.removeEventListener) {
                mq.removeEventListener('change', onMq);
            } else {
                mq.removeListener(onMq);
            }
            window.removeEventListener('theme-changed', onCustom);
        };
        // 入参 spec 在调用方应该是 stable 的；只首次注册一次监听
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, []);

    return tokens;
}
