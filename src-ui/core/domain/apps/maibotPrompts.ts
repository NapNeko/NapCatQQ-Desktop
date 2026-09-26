// 麦麦提示词模板的纯逻辑：占位符解析、保存前检查、显示名。规则和后端
// crates/ncd-appframework/src/maibot/resources/prompts/mod.rs 一致，前端拿来边打字边提示。

import type { MaiBotPromptInfo } from '../../ipc/types';

export type PromptFields = { ok: true; fields: string[] } | { ok: false; error: string };

// 照 Python string.Formatter.parse：{a.b} {a[0]} {a!r} {a:>5} 都算 a，{{ }} 是字面括号
function fieldBase(field: string): string | null {
    let end = field.length;
    let inBracket = false;
    for (let i = 0; i < field.length; i += 1) {
        const c = field[i];
        if (c === '[' && !inBracket) inBracket = true;
        else if (c === ']' && inBracket) inBracket = false;
        else if (c === '{' && !inBracket) return null;
        else if ((c === ':' || c === '!') && !inBracket) {
            end = i;
            break;
        }
    }
    return field.slice(0, end).split(/[.[]/)[0] ?? '';
}

export function promptFields(text: string): PromptFields {
    const fields: string[] = [];
    let i = 0;
    while (i < text.length) {
        const c = text[i];
        const next = text[i + 1];
        if (c === '{' && next === '{') i += 2;
        else if (c === '}' && next === '}') i += 2;
        else if (c === '}') return { ok: false, error: '有一个单独的 }，字面的花括号要写成 }}' };
        else if (c === '{') {
            let depth = 1;
            let j = i + 1;
            for (; j < text.length; j += 1) {
                if (text[j] === '{') depth += 1;
                else if (text[j] === '}') {
                    depth -= 1;
                    if (depth === 0) break;
                }
            }
            if (j >= text.length) return { ok: false, error: '有一个 { 没有配对的 }，字面的花括号要写成 {{' };
            const field = text.slice(i + 1, j);
            const base = fieldBase(field);
            if (base === null) return { ok: false, error: `参数名里不能再有 {：{${field}}` };
            fields.push(base);
            i = j + 1;
        } else i += 1;
    }
    return { ok: true, fields };
}

/** 默认模板要的参数，去重保序 */
export function promptParams(defaultContent: string): string[] {
    const parsed = promptFields(defaultContent);
    if (!parsed.ok) return [];
    return [...new Set(parsed.fields.filter(Boolean))];
}

export type PromptCheck = {
    /** null 就是能存 */
    error: string | null;
    missing: string[];
    extra: string[];
};

/**
 * 存之前的检查：和上游一样比占位符，另外拦空模板和 {}。
 * 上游放过这两样，但它加载模板时会整个失败，麦麦连启动都起不来
 */
export function checkPrompt(content: string, defaultContent: string): PromptCheck {
    if (!content.trim()) return { error: '提示词不能是空的', missing: [], extra: [] };
    const parsed = promptFields(content);
    if (!parsed.ok) return { error: parsed.error, missing: [], extra: [] };
    if (parsed.fields.some((f) => !f)) {
        return { error: '不能有空的 {}：要写参数名，字面的花括号写成 {{ }}', missing: [], extra: [] };
    }
    const want = new Set(promptParams(defaultContent));
    const got = new Set(parsed.fields);
    const missing = [...want].filter((f) => !got.has(f)).sort();
    const extra = [...got].filter((f) => !want.has(f)).sort();
    const parts: string[] = [];
    if (missing.length) parts.push(`少了 ${missing.map((f) => `{${f}}`).join('、')}`);
    if (extra.length) parts.push(`多了 ${extra.map((f) => `{${f}}`).join('、')}（麦麦给不出这些参数）`);
    return { error: parts.length ? parts.join('；') : null, missing, extra };
}

// 上游按文件名排，最常改的「回复」落在后面；这几个提到前面，其余照文件名
const PROMPT_ORDER = ['maisaka_replyer.prompt', 'maisaka_chat.prompt', 'image_description.prompt'];

export function sortPrompts<T extends Pick<MaiBotPromptInfo, 'name'>>(prompts: readonly T[]): T[] {
    const rank = (name: string) => {
        const i = PROMPT_ORDER.indexOf(name);
        return i < 0 ? PROMPT_ORDER.length : i;
    };
    return [...prompts].sort((a, b) => rank(a.name) - rank(b.name) || a.name.localeCompare(b.name));
}

export function promptDisplayName(p: Pick<MaiBotPromptInfo, 'display_name' | 'name'>): string {
    return p.display_name || p.name.replace(/\.prompt$/, '');
}

const LANGUAGE_LABELS: Readonly<Record<string, string>> = {
    'zh-CN': '简体中文',
    'en-US': 'English',
    'ja-JP': '日本語',
    ko: '한국어',
};

export function promptLanguageLabel(language: string): string {
    return LANGUAGE_LABELS[language] ?? language;
}

/** 覆盖上游在用版本时的保存目标：旧格式的覆盖没有版本号，只能新建 */
export const PROMPT_LEGACY_VERSION = 'legacy-current';
