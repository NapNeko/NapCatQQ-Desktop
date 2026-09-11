import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useAppInstanceConfig } from '../../../hooks/apps/useAppInstanceConfig';
import { pushInfoBar } from '../../../hooks/ui/globalInfoBarStore';
import { pushAppErrorBar } from '../../../hooks/apps/pushAppErrorBar';
import { issuesByPath } from '../../../core/domain/apps/karinConfig';
import { validateAstrBotConfig } from '../../../core/domain/apps/astrbotConfig';
import type {
    AppConfigError,
    AppConfigIssue,
    AppConfigWriteResult,
    AstrBotInstanceConfig,
} from '../../../core/ipc/types';

export type SaveOutcome =
    | { kind: 'saved'; result: AppConfigWriteResult }
    | { kind: 'conflict' }
    | { kind: 'invalid'; issues: AppConfigIssue[] }
    | { kind: 'error'; message: string }
    | { kind: 'noop' };

export function useAstrBotConfigForm(
    instanceId: string,
    enabled: boolean,
    instanceName: string,
    running = false,
    confId = 'default',
) {
    const remote = useAppInstanceConfig(instanceId, enabled);
    const [form, setFormState] = useState<AstrBotInstanceConfig | null>(null);
    const [pristine, setPristine] = useState<AstrBotInstanceConfig | null>(null);
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
        if (!env || env.config.framework !== 'astrbot') return;
        if (hydratedRevision.current === env.revision) return;
        if (dirtyRef.current && hydratedRevision.current !== null) return;
        hydratedRevision.current = env.revision;
        setFormState(structuredClone(env.config.data));
        setPristine(structuredClone(env.config.data));
        setServerIssues([]);
    }, [remote.envelope]);

    const setForm = useCallback((next: AstrBotInstanceConfig) => {
        setFormState(next);
        setServerIssues((prev) => (prev.length ? [] : prev));
    }, []);

    const clientIssues = useMemo(() => (form ? validateAstrBotConfig(form) : []), [form]);
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
                    config: { framework: 'astrbot', data: form },
                    baseRevision: overwrite ? null : (hydratedRevision.current ?? envelope.revision),
                    confId,
                });
                if (result.config.framework === 'astrbot') {
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
                    content: describeSave(result, running),
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
                void reload();
                return { kind: 'error', message: err.message };
            }
        },
        [clientIssues, confId, envelope, form, instanceId, instanceName, reload, running, write],
    );

    return {
        form,
        setForm,
        /** 最近一次读到 / 写成功的版本；「这个源服务端认不认」看它，不看草稿 */
        saved: pristine,
        dirty,
        errors,
        clientIssues,
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

function describeSave(r: AppConfigWriteResult, running: boolean): string {
    const parts: string[] = [];
    if (r.port_changed) parts.push('实例端口已同步');
    if (r.relinked) parts.push('已同步更新协议 Bot 侧的对接连接');
    if (r.restart_required) parts.push('改完要重启');
    else if (running) parts.push('已热生效');
    else parts.push('已写入');
    return parts.join('；');
}
