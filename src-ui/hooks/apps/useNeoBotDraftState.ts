import { useContext, useEffect, useState } from 'react';
import { NeoBotDraftContext } from './neoBotDraftContext';

export function useNeoBotDraftState<T>(key: string, initial: T | (() => T)) {
    const drafts = useContext(NeoBotDraftContext);
    const [value, setValue] = useState<T>(() =>
        drafts?.values.has(key)
            ? (drafts.values.get(key) as T)
            : typeof initial === 'function'
              ? (initial as () => T)()
              : initial,
    );
    useEffect(() => {
        drafts?.values.set(key, value);
    }, [drafts, key, value]);
    return [value, setValue] as const;
}
