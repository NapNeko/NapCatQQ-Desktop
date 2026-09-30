// 参数编辑器里的 JSON 文本 ⇄ 对象。表单和 JSON 是同一份数据的两种视图，
// 表单改一个字段就对文本做一次最小修改（setParam），文本里表单不认识的键不能丢、顺序不能乱。

import { buildFormModel } from './schemaForm';

export type ParamsParse =
    | { ok: true; value: Record<string, unknown> }
    | { ok: false; message: string; line: number; column: number };

const NOT_OBJECT_MESSAGE = '参数必须是 JSON 对象';
const INCOMPLETE_MESSAGE = 'JSON 不完整，可能少了括号或引号';

function isPlainObject(v: unknown): v is Record<string, unknown> {
    return typeof v === 'object' && v !== null && !Array.isArray(v);
}

/** 字符偏移换成 1 起算的行列，编辑器用它定位出错的位置 */
function lineColumnAt(text: string, offset: number): { line: number; column: number } {
    const end = Math.max(0, Math.min(offset, text.length));
    let line = 1;
    let lineStart = 0;
    for (let i = 0; i < end; i += 1) {
        if (text.charCodeAt(i) === 10) {
            line += 1;
            lineStart = i + 1;
        }
    }
    return { line, column: end - lineStart + 1 };
}

interface SyntaxIssue {
    offset: number;
    message: string;
}

const MAX_SCAN_DEPTH = 200;
const NUMBER_HEAD = /^-?(?:0|[1-9]\d*)(?:\.\d+)?(?:[eE][+-]?\d+)?/;

/**
 * 定位 JSON 里第一个语法错误。JSON.parse 的报错有的带位置有的不带（比如「Unexpected token ','」就没有），
 * 而编辑器要高亮出错的行，只能自己扫一遍。只在 JSON.parse 已经失败时调用，所以不追求快，只求指对地方；
 * 扫不出问题（理论上不会）返回 null，由调用方用引擎给的信息兜底。
 */
function findSyntaxError(text: string): SyntaxIssue | null {
    let i = 0;
    const fail = (offset: number, detail: string): never => {
        throw { offset, message: `JSON 语法错误：${detail}` } satisfies SyntaxIssue;
    };
    const incomplete = (): never => {
        throw { offset: text.length, message: INCOMPLETE_MESSAGE } satisfies SyntaxIssue;
    };
    const skipWs = () => {
        while (i < text.length && ' \t\n\r'.includes(text[i] as string)) i += 1;
    };

    const scanString = () => {
        i += 1; // 开头的引号
        while (i < text.length) {
            const c = text.charCodeAt(i);
            if (c === 0x22) {
                i += 1;
                return;
            }
            if (c < 0x20) fail(i, '字符串里不能直接换行，换行要写成 \\n');
            if (c === 0x5c) {
                const next = text[i + 1];
                if (next === undefined) return incomplete();
                if (next === 'u') {
                    if (!/^[0-9a-fA-F]{4}$/.test(text.slice(i + 2, i + 6))) fail(i, '\\u 后面要跟 4 位十六进制数');
                    i += 6;
                } else if ('"\\/bfnrt'.includes(next)) {
                    i += 2;
                } else {
                    fail(i, '字符串里的转义写法不对');
                }
                continue;
            }
            i += 1;
        }
        incomplete();
    };

    const scanValue = (depth: number): void => {
        if (depth > MAX_SCAN_DEPTH) fail(i, '嵌套太深');
        skipWs();
        const c = text[i];
        if (c === '{') {
            i += 1;
            skipWs();
            if (text[i] === '}') {
                i += 1;
                return;
            }
            for (;;) {
                skipWs();
                if (i >= text.length) incomplete();
                if (text[i] !== '"') fail(i, '属性名要用双引号括起来');
                scanString();
                skipWs();
                if (i >= text.length) incomplete();
                if (text[i] !== ':') fail(i, '属性名后面缺少冒号');
                i += 1;
                scanValue(depth + 1);
                skipWs();
                if (text[i] === ',') {
                    i += 1;
                    skipWs();
                    if (text[i] === '}') fail(i, '最后一项后面不能有逗号');
                    continue;
                }
                if (text[i] === '}') {
                    i += 1;
                    return;
                }
                if (i >= text.length) incomplete();
                fail(i, '这里缺少逗号或右花括号');
            }
        }
        if (c === '[') {
            i += 1;
            skipWs();
            if (text[i] === ']') {
                i += 1;
                return;
            }
            for (;;) {
                scanValue(depth + 1);
                skipWs();
                if (text[i] === ',') {
                    i += 1;
                    skipWs();
                    if (text[i] === ']') fail(i, '最后一项后面不能有逗号');
                    continue;
                }
                if (text[i] === ']') {
                    i += 1;
                    return;
                }
                if (i >= text.length) incomplete();
                fail(i, '这里缺少逗号或右方括号');
            }
        }
        if (c === '"') return scanString();
        if (c === '-' || (c !== undefined && c >= '0' && c <= '9')) {
            const m = NUMBER_HEAD.exec(text.slice(i, i + 400));
            if (!m) fail(i, '数字格式不对');
            i += (m as RegExpExecArray)[0].length;
            return;
        }
        for (const word of ['true', 'false', 'null']) {
            if (text.startsWith(word, i)) {
                i += word.length;
                return;
            }
        }
        if (i >= text.length) incomplete();
        fail(i, `意外的字符 "${text[i]}"`);
    };

    try {
        scanValue(0);
        skipWs();
        if (i < text.length) fail(i, 'JSON 后面有多余内容');
        return null;
    } catch (err) {
        if (typeof err === 'object' && err !== null && 'offset' in err && 'message' in err) return err as SyntaxIssue;
        throw err;
    }
}

/** 扫描器没扫出问题时退回引擎报错里的位置；再没有就指到文本末尾 */
function engineIssue(err: unknown, text: string): SyntaxIssue {
    const raw = err instanceof Error ? err.message : String(err);
    const pos = /position (\d+)/.exec(raw);
    return { offset: pos ? Number(pos[1]) : text.length, message: `JSON 语法错误：${raw}` };
}

export function parseParamsText(text: string): ParamsParse {
    if (text.trim() === '') return { ok: true, value: {} };
    let parsed: unknown;
    try {
        parsed = JSON.parse(text);
    } catch (err) {
        const issue = findSyntaxError(text) ?? engineIssue(err, text);
        const { line, column } = lineColumnAt(text, issue.offset);
        return { ok: false, message: issue.message, line, column };
    }
    if (!isPlainObject(parsed)) {
        // 指到第一个非空白字符，用户一眼看到是哪里不对
        const first = text.search(/\S/);
        const { line, column } = lineColumnAt(text, first < 0 ? 0 : first);
        return { ok: false, message: NOT_OBJECT_MESSAGE, line, column };
    }
    return { ok: true, value: parsed };
}

export function formatParams(value: Record<string, unknown>): string {
    return JSON.stringify(value, null, 2);
}

/** `__proto__` 这类键名走赋值会改原型，用 defineProperty 保证永远是自有属性 */
function put(target: Record<string, unknown>, key: string, value: unknown): void {
    Object.defineProperty(target, key, { value, enumerable: true, writable: true, configurable: true });
}

/**
 * 改文本里的一个参数。文本本身不合法就原样返回（表单此时是锁定的，不该走到这里）；
 * value 为 undefined 表示删掉这个键；已有的键留在原位，新键追加在末尾。
 */
export function setParam(text: string, name: string, value: unknown): string {
    const parsed = parseParamsText(text);
    if (!parsed.ok) return text;
    const current = parsed.value;
    const exists = Object.prototype.hasOwnProperty.call(current, name);
    if (value === undefined && !exists) return text;

    const next: Record<string, unknown> = {};
    for (const [k, v] of Object.entries(current)) {
        if (k === name) {
            if (value !== undefined) put(next, k, value);
        } else {
            put(next, k, v);
        }
    }
    if (!exists && value !== undefined) put(next, name, value);
    return formatParams(next);
}

/** 群号、QQ 号、消息 id 这类「要填一个真实的号」的字段 */
const ID_KINDS: ReadonlySet<string> = new Set(['group', 'friend', 'member', 'message_id']);

/**
 * 新开标签时参数编辑器里放什么：优先用文档里的第一个请求示例；
 * 没有示例就把必填项列出来（带个空值占位），用户至少知道要填哪些；都没有就是 `{}`。
 * 整数型的 id 字段不放占位：0 看着像填好了（校验也认它），危险确认里还会写成「会撤回消息 0」，
 * 干脆空着，表单上照样标「必填」；字符串 id 的空串占位留着，校验会当成没填。
 */
export function initialParamsText(
    spec: { examples: unknown[]; params_schema: Record<string, unknown> } | null,
): string {
    if (!spec) return '{}';
    const example = spec.examples.find(isPlainObject);
    if (example) return formatParams(example);

    const required = buildFormModel(spec.params_schema).fields.filter(
        (f) => f.required && !(ID_KINDS.has(f.kind) && typeof f.emptyValue === 'number'),
    );
    if (required.length === 0) return '{}';
    const value: Record<string, unknown> = {};
    for (const f of required) put(value, f.name, f.emptyValue ?? '');
    return formatParams(value);
}
