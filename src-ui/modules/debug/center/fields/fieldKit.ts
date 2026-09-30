// 参数表单控件共用的约定：props 形状，和「输入框里的草稿」怎么跟参数值同步。

import { useEffect, useState } from 'react';
import type { FormField } from '../../../../core/domain/debug/schemaForm';
import { sameJson } from '../viewHelpers';

export interface FieldProps {
    field: FormField;
    /** 参数里现在的值；undefined 表示没填这个键 */
    value: unknown;
    /** 写回参数；传 undefined 表示删掉这个键 */
    onChange: (value: unknown) => void;
    invalid: boolean;
    /** 控件自己的 id，标签的 htmlFor 指向它；「去改这个参数」也靠它聚焦 */
    inputId: string;
    /** 说明文字 / 报错的 id */
    describedBy?: string;
    disabled?: boolean;
    /** Mod-Enter 发送（内嵌 JSON 编辑器用） */
    onSubmit?: () => void;
}

/**
 * 输入框里的文字和参数值不是一回事：敲「1.」时参数已经是 1，要是拿参数值回填输入框，
 * 小数点就被吃掉了。所以输入框留一份自己的草稿：参数值是草稿转出来的那个时草稿不动，
 * 只有值从外面变了（JSON 视图里改了、套用了示例）才用新值重写草稿。
 */
export function useTextDraft(
    value: unknown,
    toText: (v: unknown) => string,
    fromText: (text: string) => unknown,
    onChange: (v: unknown) => void,
): [string, (text: string) => void] {
    const [state, setState] = useState(() => ({ text: toText(value), value }));
    let current = state;
    if (!sameJson(state.value, value)) {
        // 渲染期间据外部变化更新自己的 state，是 React 认可的「记住上一次的 props」写法
        current = { text: toText(value), value };
        setState(current);
    }
    const onText = (text: string) => {
        const next = fromText(text);
        setState({ text, value: next });
        onChange(next);
    };
    return [current.text, onText];
}

/** 值停止变化 delayMs 之后才跟上；敲字过程中不跟 */
export function useDebounced<T>(value: T, delayMs: number): T {
    const [settled, setSettled] = useState(value);
    useEffect(() => {
        if (Object.is(settled, value)) return;
        const t = setTimeout(() => setSettled(value), delayMs);
        return () => clearTimeout(t);
    }, [value, delayMs, settled]);
    return settled;
}
