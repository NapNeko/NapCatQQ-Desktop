import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useAppInstanceConfig } from '../../../hooks/apps/useAppInstanceConfig';
import { pushInfoBar } from '../../../hooks/ui/globalInfoBarStore';
import { pushAppErrorBar } from '../../../hooks/apps/pushAppErrorBar';
import { issuesByPath } from '../../../core/domain/apps/karinConfig';
import { validateMaiBotConfig } from '../../../core/domain/apps/maibotConfig';
import type {
    AppConfigError,
    AppConfigIssue,
    AppConfigWriteResult,
    MaiBotInstanceConfig,
} from '../../../core/ipc/types';
import type { SaveOutcome } from './useNoneBot2ConfigForm';

export function useMaiBotConfigForm(instanceId: string, enabled: boolean, instanceName: string) {
    const remote = useAppInstanceConfig(instanceId, enabled);
    const [form, setFormState] = useState<MaiBotInstanceConfig | null>(null);
    const [pristine, setPristine] = useState<MaiBotInstanceConfig | null>(null);
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
        if (!env || env.config.framework !== 'maibot') return;
        if (hydratedRevision.current === env.revision) return;
        if (dirtyRef.current && hydratedRevision.current !== null) return;
        hydratedRevision.current = env.revision;
        setFormState(structuredClone(env.config.data));
        setPristine(structuredClone(env.config.data));
        setServerIssues([]);
    }, [remote.envelope]);

    const setForm = useCallback((next: MaiBotInstanceConfig) => {
        setFormState(next);
        setServerIssues((prev) => (prev.length ? [] : prev));
    }, []);

    const clientIssues = useMemo(() => (form ? validateMaiBotConfig(form) : []), [form]);
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
                    config: { framework: 'maibot', data: form },
                    baseRevision: overwrite ? null : (hydratedRevision.current ?? envelope.revision),
                });
                if (result.config.framework === 'maibot') {
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
                void reload();
                return { kind: 'error', message: err.message };
            }
        },
        [clientIssues, envelope, form, instanceId, instanceName, reload, write],
    );

    return {
        form,
        saved: pristine,
        setForm,
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

function describeSave(r: AppConfigWriteResult): string {
    const parts: string[] = [];
    if (r.port_changed) parts.push('实例端口已同步');
    // 麦麦和适配器都热加载配置；只有端口、日志、插件运行时这些启动时读的要重启
    if (r.restart_required) parts.push('端口、日志这类启动时读的设置要重启麦麦才生效');
    return parts.join('；') || '已保存';
}
