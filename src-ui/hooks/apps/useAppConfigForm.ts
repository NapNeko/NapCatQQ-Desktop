// 应用端类型化配置的表单状态：灌入 / 脏标记 / 即时校验 / 保存分流（冲突 / 校验 / 其它），四个框架共用，
// 框架之间的差别（怎么校验、保存后补哪句话、写哪个档案）由 ConfigFormSpec 带进来。
//
// 灌表单的规则与 Bot 配置页一致：未 dirty 时随服务端版本更新；有未保存改动时不覆盖用户输入。

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useAppInstanceConfig } from './useAppInstanceConfig';
import { pushInfoBar } from '../ui/globalInfoBarStore';
import { pushAppErrorBar } from './pushAppErrorBar';
import {
    configDataOf,
    configSaveSummary,
    issuesByPath,
    wrapConfigData,
    type AppConfigData,
    type AppConfigFramework,
    type ConfigFormSpec,
} from '../../core/domain/apps/appConfigForm';
import type { AppConfigError, AppConfigIssue, AppConfigWriteResult } from '../../core/ipc/types';

export type SaveOutcome =
    | { kind: 'saved'; result: AppConfigWriteResult }
    | { kind: 'conflict' }
    | { kind: 'invalid'; issues: AppConfigIssue[] }
    | { kind: 'error'; message: string }
    | { kind: 'noop' };

/** running 只影响保存成功条那句怎么说（AstrBot 在跑时是「已热生效」） */
export function useAppConfigForm<F extends AppConfigFramework>(
    spec: ConfigFormSpec<F>,
    instanceId: string,
    instanceName: string,
    running = false,
) {
    const { framework, validate, saveHint, confId } = spec;
    const remote = useAppInstanceConfig(instanceId);
    const [form, setFormState] = useState<AppConfigData<F> | null>(null);
    const [pristine, setPristine] = useState<AppConfigData<F> | null>(null);
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
        const data = env ? configDataOf(env.config, framework) : null;
        if (!env || !data) return;
        if (hydratedRevision.current === env.revision) return;
        if (dirtyRef.current && hydratedRevision.current !== null) return;
        hydratedRevision.current = env.revision;
        setFormState(structuredClone(data));
        setPristine(structuredClone(data));
        setServerIssues([]);
    }, [framework, remote.envelope]);

    const setForm = useCallback((next: AppConfigData<F>) => {
        setFormState(next);
        // 用户一改就清掉服务端回填的错误，避免改对了还挂着红字
        setServerIssues((prev) => (prev.length ? [] : prev));
    }, []);

    const clientIssues = useMemo(() => (form ? validate(form) : []), [form, validate]);
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
                    config: wrapConfigData(framework, form),
                    // 必须用灌表时的版本号：对接会改 .env，query 刷新后
                    // envelope.revision 变了但表单还是旧内容，拿新版本号保存会把对接键盖掉。
                    baseRevision: overwrite ? null : (hydratedRevision.current ?? envelope.revision),
                    confId,
                });
                const saved = configDataOf(result.config, framework);
                if (saved) {
                    hydratedRevision.current = result.revision;
                    setFormState(structuredClone(saved));
                    setPristine(structuredClone(saved));
                }
                setServerIssues([]);
                setConflict(false);
                pushInfoBar({
                    key: `app-config-saved:${instanceId}`,
                    tone: result.restart_required ? 'warning' : 'success',
                    title: `${instanceName} 配置已保存`,
                    content: configSaveSummary(result, saveHint(result, running)),
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
        [clientIssues, confId, envelope, form, framework, instanceId, instanceName, reload, running, saveHint, write],
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
