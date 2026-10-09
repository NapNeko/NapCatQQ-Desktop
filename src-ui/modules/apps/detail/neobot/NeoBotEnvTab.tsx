import { useState } from 'react';
import { useNeoBotDraftState } from '../../../../hooks/apps/useNeoBotDraftState';
import { Button, TextField } from '../../../../shared/ui';
import { asRecord } from '../../../../core/domain/apps/neobotPanel';
import {
    record,
    records,
    text,
    type PanelObject,
} from '../../../../core/domain/apps/neobotWorkspace';
import { usePanelJson } from '../../../../hooks/apps/useNeoBotPanel';
import { useNeoBotAction } from '../../../../hooks/apps/useNeoBotAction';
import { ConfirmAction, PanelPage, PanelSection, type NeoBotPageProps } from './workspaceParts';

function EnvironmentEditor({ instanceId, doc }: { instanceId: string; doc: PanelObject }) {
    const action = useNeoBotAction(instanceId);
    const [base, setBase] = useNeoBotDraftState('env:base', doc);
    const [updates, setUpdates] = useNeoBotDraftState<Record<string, string>>('env:updates', {});
    const [deletes, setDeletes] = useNeoBotDraftState<string[]>('env:deletes', []);
    const [filter, setFilter] = useState('');
    const [custom, setCustom] = useNeoBotDraftState('env:custom', { key: '', value: '' });
    const [platform, setPlatform] = useNeoBotDraftState('env:platform', {
        name: '',
        url: '',
        api_key: '',
    });
    const dirty = Object.keys(updates).length > 0 || deletes.length > 0;
    const writable = base.can_manage !== false && base.editable !== false;
    const apply = (next: PanelObject) => {
        setBase(next);
        setUpdates({});
        setDeletes([]);
    };
    const save = async () => {
        const next = await action.run({
            path: '/api/config/env',
            body: { revision: base.revision, updates, deletes, reload: true },
        });
        if (next) apply(next);
    };
    return (
        <div className="flex flex-col gap-4">
            <PanelSection title="API 供应商">
                <div className="grid gap-3 sm:grid-cols-3">
                    <TextField
                        label="供应商名称"
                        value={platform.name}
                        onValueChange={(name) => setPlatform({ ...platform, name })}
                    />
                    <TextField
                        label="API 地址"
                        value={platform.url}
                        onValueChange={(url) => setPlatform({ ...platform, url })}
                    />
                    <TextField
                        label="API Key"
                        type="password"
                        autoComplete="off"
                        value={platform.api_key}
                        onValueChange={(api_key) => setPlatform({ ...platform, api_key })}
                    />
                </div>
                <Button
                    className="mt-3"
                    size="sm"
                    variant="primary"
                    disabled={
                        !writable ||
                        dirty ||
                        action.isPending ||
                        !platform.name.trim() ||
                        !platform.url.trim()
                    }
                    onClick={() =>
                        void action
                            .run({
                                path: '/api/config/env/platform',
                                body: { ...platform, revision: base.revision },
                            })
                            .then((next) => {
                                if (next) {
                                    apply(next);
                                    setPlatform({ name: '', url: '', api_key: '' });
                                }
                            })
                    }
                >
                    添加 / 更新供应商
                </Button>
                <div className="mt-3 flex flex-wrap gap-2">
                    {records(base.platforms).map((p) => (
                        <Button
                            key={text(p.name)}
                            size="sm"
                            variant="ghost"
                            onClick={() =>
                                setPlatform({ name: text(p.name), url: text(p.url), api_key: '' })
                            }
                        >
                            {text(p.name)} · {p.has_key ? '密钥已设' : '未设密钥'}
                        </Button>
                    ))}
                </div>
            </PanelSection>
            <PanelSection
                title="环境变量"
                actions={
                    <>
                        <ConfirmAction
                            label="重新读取"
                            description="放弃未保存的变量修改？"
                            disabled={action.isPending}
                            onConfirm={() =>
                                void action
                                    .run({ path: '/api/config/env', method: 'GET', quiet: true })
                                    .then((next) => {
                                        if (next) apply(next);
                                    })
                            }
                        />
                        <Button
                            size="sm"
                            variant="primary"
                            disabled={!writable || !dirty || action.isPending}
                            onClick={() => void save()}
                        >
                            保存并重载
                        </Button>
                    </>
                }
            >
                <p className="mb-3 text-xs text-text-tertiary">
                    密钥留空保留原值；删除变量需明确确认。
                </p>
                <TextField label="筛选变量" value={filter} onValueChange={setFilter} />
                <div className="mt-4 flex flex-col gap-3">
                    {records(base.items)
                        .filter((item) =>
                            text(item.key).toLowerCase().includes(filter.toLowerCase()),
                        )
                        .map((item) => {
                            const key = text(item.key);
                            const removed = deletes.includes(key);
                            return (
                                <div key={key} className="flex items-end gap-2">
                                    <TextField
                                        className="flex-1"
                                        label={
                                            <span title={text(item.description)}>
                                                {key}
                                                {removed ? '（待删除）' : ''}
                                            </span>
                                        }
                                        type={item.sensitive ? 'password' : 'text'}
                                        autoComplete="off"
                                        disabled={!writable || removed || action.isPending}
                                        value={
                                            updates[key] ?? (item.sensitive ? '' : text(item.value))
                                        }
                                        placeholder={
                                            item.sensitive && item.has_value
                                                ? '已设置，留空不修改'
                                                : '未设置'
                                        }
                                        onValueChange={(value) =>
                                            setUpdates({ ...updates, [key]: value })
                                        }
                                    />
                                    {removed ? (
                                        <Button
                                            size="sm"
                                            variant="ghost"
                                            onClick={() =>
                                                setDeletes(deletes.filter((x) => x !== key))
                                            }
                                        >
                                            撤销删除
                                        </Button>
                                    ) : (
                                        item.in_file === true && (
                                            <ConfirmAction
                                                label="删除变量"
                                                description={`删除 ${key}，保存后生效。`}
                                                disabled={!writable || action.isPending}
                                                onConfirm={() => {
                                                    setDeletes([...deletes, key]);
                                                    setUpdates((prev) => {
                                                        const next = { ...prev };
                                                        delete next[key];
                                                        return next;
                                                    });
                                                }}
                                            />
                                        )
                                    )}
                                </div>
                            );
                        })}
                </div>
                <div className="mt-4 grid gap-3 sm:grid-cols-[1fr_1fr_auto]">
                    <TextField
                        label="新增变量名"
                        value={custom.key}
                        onValueChange={(key) => setCustom({ ...custom, key })}
                    />
                    <TextField
                        label="值"
                        type="password"
                        autoComplete="off"
                        value={custom.value}
                        onValueChange={(value) => setCustom({ ...custom, value })}
                    />
                    <Button
                        className="self-end"
                        size="sm"
                        variant="secondary"
                        disabled={
                            !writable ||
                            !/^[A-Za-z_][A-Za-z0-9_]*$/.test(custom.key) ||
                            action.isPending
                        }
                        onClick={() => {
                            setUpdates({ ...updates, [custom.key]: custom.value });
                            setCustom({ key: '', value: '' });
                        }}
                    >
                        加入草稿
                    </Button>
                </div>
                {Object.keys(updates)
                    .filter((key) => !records(base.items).some((i) => text(i.key) === key))
                    .map((key) => (
                        <p key={key} className="mt-2 text-xs text-text-secondary">
                            待新增：{key}
                        </p>
                    ))}
            </PanelSection>
        </div>
    );
}

export function NeoBotEnvTab({ instanceId, onGoTab }: NeoBotPageProps) {
    const query = usePanelJson(instanceId, 'env', '/api/config/env', asRecord);
    return (
        <PanelPage query={query} onGoTab={onGoTab}>
            {(doc) => (
                <EnvironmentEditor key={instanceId} instanceId={instanceId} doc={record(doc)} />
            )}
        </PanelPage>
    );
}
