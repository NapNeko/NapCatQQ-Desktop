// 光标跟随文本提交同步落位，避免下一帧把用户已经输入的新文字插到错误位置。
import { useCallback, useLayoutEffect, useRef, type RefObject } from 'react';

export function useCommittedCaret(input: RefObject<HTMLTextAreaElement>, text: string) {
    const pending = useRef<{ text: string; position: number } | null>(null);
    useLayoutEffect(() => {
        const selection = pending.current;
        if (!selection) return;
        pending.current = null;
        if (selection.text !== text) return;
        input.current?.focus({ preventScroll: true });
        input.current?.setSelectionRange(selection.position, selection.position);
    }, [input, text]);
    return useCallback((nextText: string, position: number) => {
        pending.current = { text: nextText, position };
    }, []);
}
