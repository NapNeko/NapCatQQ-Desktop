// Karin 类型化配置的表单状态：灌入 / 脏标记 / 即时校验 / 保存分流（冲突 / 校验 / 其它）。
//
// 灌表单的规则与 Bot 配置页一致：未 dirty 时随服务端版本更新；有未保存改动时不覆盖用户输入。

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useAppInstanceConfig } from '../../../hooks/apps/useAppInstanceConfig';
import { pushInfoBar } from '../../../hooks/ui/globalInfoBarStore';
import { pushAppErrorBar } from '../../../hooks/apps/pushAppErrorBar';
import { issuesByPath, validateKarinConfig } from '../../../core/domain/apps/karinConfig';
import type {
    AppConfigError,
    AppConfigIssue,
    AppConfigWriteResult,
    KarinInstanceConfig,
} from '../../../core/ipc/types';

export type SaveOutcome =
    | { kind: 'saved'; result: AppConfigWriteResult }
    | { kind: 'conflict' }
    | { kind: 'invalid'; issues: AppConfigIssue[] }
    | { kind: 'error'; message: string }
    | { kind: 'noop' };

export function useKarinConfigForm(instanceId: string, enabled: boolean, instanceName: string) {
    const remote = useAppInstanceConfig(instanceId, enabled);
    const [form, setFormState] = useState<KarinInstanceConfig | null>(null);
    const [pristine, setPristine] = useState<KarinInstanceConfig | null>(null);
    const [serverIssues, setServerIssues] = useState<AppConfigIssue[]>([]);
    const [conflict, setConflict] = useState(false);
    const hydratedRevision = useRef<string | null>(null);

    useEffect(() => {
        hydratedRevision.current = null;
        setFormState(null);
        setPristine(null);
        setServerIssues([]);
        setConflict(false);
    }, [instanceId]);

    const dirty = useMemo(
        () => !!form && !!pristine && JSON.stringify(form) !== JSON.stringify(pristine),
        [form, pristine],
    );
    const dirtyRef = useRef(dirty);
    dirtyRef.current = dirty;

    useEffect(() => {
        if (!remote.error) return;
        pushAppErrorBar({
            key: `app-config-load:${instanceId}`,
            title: '读取配置失败',
            raw: remote.error.message,
        });
    }, [instanceId, remote.error]);

    useEffect(() => {
        const env = remote.envelope;
        if (!env || env.config.framework !== 'karin') return;
        if (hydratedRevision.current === env.revision) return;
        if (dirtyRef.current && hydratedRevision.current !== null) return;
        hydratedRevision.current = env.revision;
        setFormState(structuredClone(env.config.data));
        setPristine(structuredClone(env.config.data));
        setServerIssues([]);
    }, [remote.envelope]);

    const setForm = useCallback((next: KarinInstanceConfig) => {
        setFormState(next);
        // 用户一改就清掉服务端回填的错误，避免改对了还挂着红字
        setServerIssues((prev) => (prev.length ? [] : prev));
    }, []);

    const clientIssues = useMemo(() => (form ? validateKarinConfig(form) : []), [form]);
    const errors = useMemo(
        () => issuesByPath([...serverIssues, ...clientIssues]),
        [serverIssues, clientIssues],
    );

    const reset = useCallback(() => {
        if (pristine) setFormState(structuredClone(pristine));
        setServerIssues([]);
    }, [pristine]);

    const reload = remote.reload;
    const write = remote.write;
    const envelope = remote.envelope;

    /** 丢弃本地改动，拉最新 */
    const reloadDiscard = useCallback(async () => {
        hydratedRevision.current = null;
        setPristine(null);
        setConflict(false);
        await reload();
    }, [reload]);

    const dismissConflict = useCallback(() => setConflict(false), []);

    const save = useCallback(
        async (overwrite = false): Promise<SaveOutcome> => {
            if (!form || !envelope) return { kind: 'noop' };
            if (clientIssues.length) {
                pushInfoBar({
                    key: `app-config-invalid:${instanceId}`,
                    tone: 'danger',
                    title: `${clientIssues.length} 处填写有误`,
                    content: clientIssues
                        .slice(0, 3)
                        .map((i) => `${i.path}: ${i.message}`)
                        .join('；'),
                    autoDismissMs: 5000,
                });
                return { kind: 'invalid', issues: clientIssues };
            }
            try {
                const result = await write({
                    config: { framework: 'karin', data: form },
                    // 必须用灌表时的版本号：对接会改 .env，query 刷新后
                    // envelope.revision 变了但表单还是旧内容，拿新版本号保存会把对接键盖掉。
                    baseRevision: overwrite ? null : (hydratedRevision.current ?? envelope.revision),
                });
                if (result.config.framework === 'karin') {
                    hydratedRevision.current = result.revision;
                    setFormState(structuredClone(result.config.data));
                    setPristine(structuredClone(result.config.data));
                }
                setServerIssues([]);
                setConflict(false);
                pushInfoBar({
                    key: `app-config-saved:${instanceId}`,
                    tone: result.restart_required ? 'warning' : 'success',
                    title: `${instanceName} 配置已保存`,
                    content: describeSave(result),
                    autoDismissMs: result.restart_required ? 8000 : 4000,
                });
                return { kind: 'saved', result };
            } catch (e) {
                const err = e as AppConfigError;
                if (err.kind === 'conflict') {
                    setConflict(true);
                    return { kind: 'conflict' };
                }
                if (err.kind === 'invalid') {
                    setServerIssues(err.issues);
                    pushInfoBar({
                        key: `app-config-invalid:${instanceId}`,
                        tone: 'danger',
                        title: '后端校验未通过',
                        content: err.issues.map((i) => `${i.path}: ${i.message}`).join('；') || err.message,
                        autoDismissMs: 6000,
                    });
                    return { kind: 'invalid', issues: err.issues };
                }
                pushAppErrorBar({
                    key: `app-config-save-failed:${instanceId}`,
                    title: '保存失败',
                    raw: err.message,
                });
                // 写可能部分成功（例如重新对接失败），拉一次最新避免本地版本号过期
                void reload();
                return { kind: 'error', message: err.message };
            }
        },
        [clientIssues, envelope, form, instanceId, instanceName, reload, write],
    );

    return {
        form,
        setForm,
        pristine,
        dirty,
        errors,
        clientIssues,
        envelope: remote.envelope,
        isLoading: remote.isLoading,
        loadError: remote.error,
        saving: remote.isWriting,
        save,
        reset,
        reloadDiscard,
        conflict,
        dismissConflict,
    };
}

function describeSave(r: AppConfigWriteResult): string {
    const parts: string[] = [];
    if (r.port_changed) parts.push('实例端口已同步');
    if (r.relinked) parts.push('已同步更新协议 Bot 侧的对接连接');
    if (r.restart_required) parts.push('有改动需重启实例后生效');
    else parts.push('Karin 会自动热加载，无需重启');
    return parts.join('；');
}
