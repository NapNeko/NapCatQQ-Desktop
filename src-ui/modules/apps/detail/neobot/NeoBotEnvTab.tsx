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
import {
    ConfirmAction,
    InlineNote,
    PanelEmpty,
    PanelPage,
    PanelSection,
    type NeoBotPageProps,
} from './workspaceParts';

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
    const writable = base.can_manage !== false && base.editable !== false && base.readonly !== true;
    const platforms = records(base.platforms);
    const items = records(base.items);
    const visible = items.filter((item) =>
        text(item.key).toLowerCase().includes(filter.trim().toLowerCase()),
    );
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
            {!writable && <InlineNote tone="warning">当前会话只有查看权限</InlineNote>}
            <PanelSection title="API 供应商" meta={`已配置 ${platforms.length} 个`}>
                <div className="mb-4 divide-y divide-border-subtle">
                    {platforms.map((p) => (
                        <div
                            key={text(p.name)}
                            className="flex min-w-0 flex-wrap items-center justify-between gap-3 py-3 first:pt-0"
                        >
                            <div className="min-w-0 flex-1">
                                <p className="truncate text-xs font-medium text-text">
                                    {text(p.name)}
                                </p>
                                <p className="mt-1 truncate text-2xs text-text-tertiary">
                                    {text(p.url) || '未设置 API 地址'}
                                </p>
                            </div>
                            <span
                                className={`text-2xs ${p.has_key ? 'text-success' : 'text-warning'}`}
                            >
                                {p.has_key ? '密钥已设置' : '未设置密钥'}
                            </span>
                            <Button
                                size="sm"
                                variant="ghost"
                                disabled={!writable || action.isPending}
                                onClick={() =>
                                    setPlatform({
                                        name: text(p.name),
                                        url: text(p.url),
                                        api_key: '',
                                    })
                                }
                            >
                                编辑供应商
                            </Button>
                        </div>
                    ))}
                    {!platforms.length && (
                        <PanelEmpty>还没有供应商，在下方填写名称和 API 地址添加</PanelEmpty>
                    )}
                </div>
                <div className="grid gap-3 border-t border-border-subtle pt-4 sm:grid-cols-2">
                    <TextField
                        label="供应商名称"
                        disabled={!writable || action.isPending}
                        value={platform.name}
                        onValueChange={(name) => setPlatform({ ...platform, name })}
                    />
                    <TextField
                        label="API 地址"
                        disabled={!writable || action.isPending}
                        value={platform.url}
                        onValueChange={(url) => setPlatform({ ...platform, url })}
                    />
                    <TextField
                        label="API Key"
                        className="sm:col-span-2"
                        disabled={!writable || action.isPending}
                        type="password"
                        autoComplete="off"
                        value={platform.api_key}
                        onValueChange={(api_key) => setPlatform({ ...platform, api_key })}
                    />
                </div>
                <Button
                    className="mt-3"
                    size="sm"
                    variant="secondary"
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
                {dirty && (
                    <div className="mt-3">
                        <InlineNote tone="warning">
                            先保存或重新读取环境变量，再修改供应商
                        </InlineNote>
                    </div>
                )}
            </PanelSection>
            <PanelSection
                title="环境变量"
                meta={
                    dirty
                        ? `${Object.keys(updates).length} 项修改，${deletes.length} 项待删除`
                        : `共 ${items.length} 项`
                }
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
                <div className="mb-4">
                    <InlineNote>密钥留空保留原值；删除变量在保存后生效</InlineNote>
                </div>
                <TextField label="筛选变量" value={filter} onValueChange={setFilter} />
                <div className="mt-4 grid gap-4 lg:grid-cols-2">
                    {visible.map((item) => {
                        const key = text(item.key);
                        const removed = deletes.includes(key);
                        return (
                            <div key={key} className="flex min-w-0 items-end gap-2">
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
                                    value={updates[key] ?? (item.sensitive ? '' : text(item.value))}
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
                                        disabled={!writable || action.isPending}
                                        onClick={() => setDeletes(deletes.filter((x) => x !== key))}
                                    >
                                        撤销删除
                                    </Button>
                                ) : (
                                    item.in_file === true && (
                                        <ConfirmAction
                                            label="删除变量"
                                            variant="danger"
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
                    {!visible.length && (
                        <div className="lg:col-span-2">
                            <PanelEmpty>
                                {filter.trim()
                                    ? '没有匹配的变量，换个关键词试试'
                                    : '还没有环境变量，可在下方添加'}
                            </PanelEmpty>
                        </div>
                    )}
                </div>
                <div className="mt-5 grid gap-3 border-t border-border-subtle pt-4 sm:grid-cols-[1fr_1fr_auto]">
                    <TextField
                        label="新增变量名"
                        disabled={!writable || action.isPending}
                        value={custom.key}
                        onValueChange={(key) => setCustom({ ...custom, key })}
                    />
                    <TextField
                        label="值"
                        disabled={!writable || action.isPending}
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
                            setDeletes(deletes.filter((key) => key !== custom.key));
                            setCustom({ key: '', value: '' });
                        }}
                    >
                        加入草稿
                    </Button>
                </div>
                {Object.keys(updates)
                    .filter((key) => !items.some((i) => text(i.key) === key))
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
