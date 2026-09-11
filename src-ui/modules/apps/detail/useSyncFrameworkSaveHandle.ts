import { useEffect, useRef } from 'react';
import type { FrameworkSaveHandle } from './frameworkUi';

type SaveHandleSource = {
    dirty: boolean;
    saving: boolean;
    clientIssues: { length: number };
    save: FrameworkSaveHandle['save'];
    reset: FrameworkSaveHandle['reset'];
    conflict: boolean;
    dismissConflict: FrameworkSaveHandle['dismissConflict'];
    reloadDiscard: FrameworkSaveHandle['reloadDiscard'];
};

/** 只同步 dirty/冲突等值；save/reset 走 ref，避免每渲 setState 饿死侧栏并发导航。 */
export function useSyncFrameworkSaveHandle(
    onSaveHandle: (handle: FrameworkSaveHandle | null) => void,
    form: SaveHandleSource,
) {
    const onSaveHandleRef = useRef(onSaveHandle);
    onSaveHandleRef.current = onSaveHandle;
    const fnsRef = useRef({
        save: form.save,
        reset: form.reset,
        dismissConflict: form.dismissConflict,
        reloadDiscard: form.reloadDiscard,
    });
    fnsRef.current = {
        save: form.save,
        reset: form.reset,
        dismissConflict: form.dismissConflict,
        reloadDiscard: form.reloadDiscard,
    };

    useEffect(() => {
        onSaveHandleRef.current({
            dirty: form.dirty,
            saving: form.saving,
            issueCount: form.clientIssues.length,
            save: (overwrite) => fnsRef.current.save(overwrite),
            reset: () => fnsRef.current.reset(),
            conflict: form.conflict,
            dismissConflict: () => fnsRef.current.dismissConflict(),
            reloadDiscard: () => fnsRef.current.reloadDiscard(),
        });
    }, [form.conflict, form.dirty, form.saving, form.clientIssues.length]);

    useEffect(() => () => onSaveHandleRef.current(null), []);
}
