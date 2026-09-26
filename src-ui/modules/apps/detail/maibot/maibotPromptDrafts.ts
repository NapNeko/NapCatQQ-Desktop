// 提示词没保存的草稿，按「语言/文件名」记。放在详情页这一层：切到别的页再回来还在，列表和侧栏能标出来。

import { useCallback, useMemo, useState } from 'react';

export interface PromptDrafts {
    get: (key: string) => string | undefined;
    set: (key: string, content: string) => void;
    clear: (key: string) => void;
    keys: readonly string[];
}

export const promptDraftKey = (language: string, name: string) => `${language}/${name}`;

export function usePromptDrafts(): PromptDrafts {
    const [drafts, setDrafts] = useState<ReadonlyMap<string, string>>(() => new Map());
    const set = useCallback((key: string, content: string) => {
        setDrafts((prev) => (prev.get(key) === content ? prev : new Map(prev).set(key, content)));
    }, []);
    const clear = useCallback((key: string) => {
        setDrafts((prev) => {
            if (!prev.has(key)) return prev;
            const next = new Map(prev);
            next.delete(key);
            return next;
        });
    }, []);
    return useMemo(
        () => ({ get: (key: string) => drafts.get(key), set, clear, keys: [...drafts.keys()] }),
        [drafts, set, clear],
    );
}
