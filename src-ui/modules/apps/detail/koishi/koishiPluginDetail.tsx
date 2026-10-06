// 插件页右侧：选中节点的详情。头部一张卡（是谁、开没开、能做什么），下面是按 schema 画的配置、
// 插件自己写的使用说明、加载条件。所有改动都是在整棵树上换出新的一份交回去，走底部保存条。

import { useMemo, useState } from 'react';
import { BookOpen, Copy, Folder, FolderInput, Link2, Puzzle, Trash2 } from 'lucide-react';
import {
    Badge,
    Button,
    FormSection,
    JsonCodeEditor,
    Popover,
    PopoverClose,
    PopoverContent,
    PopoverTrigger,
    SimpleMarkdown,
    Switch,
    TextField,
} from '../../../../shared/ui';
import { cn } from '../../../../shared/utils/cn';
import {
    KOISHI_CORE_PLUGINS,
    appendTo,
    cloneNode,
    groupChoices,
    isGroup,
    isLinkNode,
    moveTo,
    nodeKey,
    nodeLabel,
    replaceAt,
    walk,
    type NodePath,
} from '../../../../core/domain/apps/koishiConfig';
import {
    hydrateSchema,
    materializeConfig,
    missingRequired,
    renderable,
    simplifyConfig,
    toJsonSchema,
    type SNode,
} from '../../../../core/domain/apps/koishiSchema';
import { useKoishiPluginSchema } from '../../../../hooks/apps/useKoishiRuntime';
import { PaneLoading } from '../PaneStatus';
import { Notice } from './Notice';
import { SchemasteryForm } from './SchemasteryForm';
import type { KoishiInstanceConfig, KoishiPluginNode } from '../../../../core/ipc/types';

export interface DetailProps {
    instanceId: string;
    config: KoishiInstanceConfig;
    path: NodePath;
    node: KoishiPluginNode;
    /** 自己和上级分组都开着 */
    live: boolean;
    onChange: (next: KoishiInstanceConfig) => void;
    onSelect: (key: string | null) => void;
    disabled?: boolean;
}

function IconButton({
    label,
    onClick,
    disabled,
    danger,
    children,
}: {
    label: string;
    onClick?: () => void;
    disabled?: boolean;
    danger?: boolean;
    children: React.ReactNode;
}) {
    return (
        <button
            type="button"
            title={label}
            aria-label={label}
            disabled={disabled}
            onClick={onClick}
            className={cn(
                'inline-flex h-8 w-8 items-center justify-center rounded-md text-text-tertiary transition-colors disabled:pointer-events-none disabled:opacity-40',
                danger
                    ? 'hover:bg-danger-soft hover:text-danger'
                    : 'hover:bg-inset hover:text-text',
            )}
        >
            {children}
        </button>
    );
}

function MoveButton({ config, path, node, onChange, disabled }: DetailProps) {
    const groups = groupChoices(config).filter((g) => {
        if (!isGroup(node)) return true;
        return (
            g.ident !== node.ident &&
            !walk(node.children).some((c) => isGroup(c) && c.ident === g.ident)
        );
    });
    return (
        <Popover>
            <PopoverTrigger asChild>
                <button
                    type="button"
                    title="移到分组"
                    aria-label="移到分组"
                    disabled={disabled}
                    className="inline-flex h-8 w-8 items-center justify-center rounded-md text-text-tertiary transition-colors hover:bg-inset hover:text-text disabled:opacity-40"
                >
                    <FolderInput size={15} />
                </button>
            </PopoverTrigger>
            <PopoverContent align="end" className="w-56 p-1.5">
                <p className="px-2 pb-1.5 pt-1 text-2xs text-text-tertiary">移到分组</p>
                <ul className="flex max-h-72 flex-col overflow-y-auto">
                    {groups.map((g) => (
                        <li key={g.ident || 'root'}>
                            <PopoverClose asChild>
                                <button
                                    type="button"
                                    className="flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-[13px] text-text-secondary hover:bg-inset hover:text-text"
                                    onClick={() => onChange(moveTo(config, path, g.ident))}
                                >
                                    <Folder size={13} className="shrink-0 text-text-tertiary" />
                                    <span className="truncate">{g.label}</span>
                                </button>
                            </PopoverClose>
                        </li>
                    ))}
                </ul>
            </PopoverContent>
        </Popover>
    );
}

/** 头部卡片：图标、名字、包、状态、启用开关和操作 */
function DetailHeader(props: DetailProps & { subtitle: React.ReactNode }) {
    const { config, path, node, live, onChange, onSelect, disabled, subtitle } = props;
    const group = isGroup(node);
    const locked = KOISHI_CORE_PLUGINS.has(node.name) || isLinkNode(node);
    const Icon = group ? Folder : isLinkNode(node) ? Link2 : Puzzle;
    const state = !node.enabled ? '已停用' : live ? '运行时载入' : '所在分组停用';
    return (
        <div className="flex flex-col gap-4 rounded-lg border border-border-subtle bg-surface px-5 py-4 shadow-card">
            <div className="flex items-start gap-3.5">
                <span
                    className={cn(
                        'flex h-10 w-10 shrink-0 items-center justify-center rounded-md',
                        live ? 'bg-brand-soft text-brand' : 'bg-inset text-text-tertiary',
                    )}
                >
                    <Icon size={18} />
                </span>
                <div className="min-w-0 flex-1">
                    <h3 className="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1 font-display text-[16px] font-semibold leading-snug text-text">
                        <span className="truncate">{group ? nodeLabel(node) : node.name}</span>
                        <span className="font-mono text-2xs font-normal text-text-disabled">
                            {nodeKey(node)}
                        </span>
                        {KOISHI_CORE_PLUGINS.has(node.name) && <Badge tone="neutral">核心</Badge>}
                        {isLinkNode(node) && <Badge tone="brand">桌面端对接</Badge>}
                        {typeof node.meta.$if === 'string' && <Badge tone="info">按条件载入</Badge>}
                    </h3>
                    <p className="mt-1 truncate text-xs text-text-tertiary">{subtitle}</p>
                </div>
                <div className="flex shrink-0 items-center gap-0.5">
                    {!group && (
                        <IconButton
                            label="再开一份（比如同一个适配器接第二个账号）"
                            disabled={disabled || isLinkNode(node)}
                            onClick={() => {
                                const copy = cloneNode(config, node);
                                onChange(appendTo(config, path.slice(0, -1), copy));
                                onSelect(nodeKey(copy));
                            }}
                        >
                            <Copy size={15} />
                        </IconButton>
                    )}
                    <MoveButton {...props} />
                    <IconButton
                        label={
                            locked
                                ? 'Koishi 和桌面端要用它，不能删'
                                : group
                                  ? '删掉分组（连同里面的）'
                                  : '从插件树删掉'
                        }
                        danger
                        disabled={disabled || locked}
                        onClick={() => {
                            onChange(replaceAt(config, path, () => null));
                            onSelect(null);
                        }}
                    >
                        <Trash2 size={15} />
                    </IconButton>
                </div>
            </div>
            <div className="flex items-center justify-between gap-4 border-t border-border-subtle/70 pt-3.5">
                <div className="min-w-0">
                    <p className="text-[13px] font-medium text-text">
                        {group ? '启用这个分组' : '启用'}
                    </p>
                    <p className="mt-0.5 text-2xs text-text-tertiary">
                        {group
                            ? '停用后里面的插件一起停，各自的开关不变'
                            : locked && node.enabled
                              ? 'Koishi 和桌面端要用它，不能停'
                              : state}
                    </p>
                </div>
                <Switch
                    aria-label={group ? '启用这个分组' : '启用'}
                    checked={node.enabled}
                    disabled={disabled || (locked && node.enabled)}
                    onCheckedChange={(enabled) =>
                        onChange(replaceAt(config, path, (n) => ({ ...n, enabled })))
                    }
                />
            </div>
        </div>
    );
}

function MetaSection({ node }: { node: KoishiPluginNode }) {
    const shown = Object.entries(node.meta).filter(([k]) => k === '$if' || k === '$filter');
    if (shown.length === 0) return null;
    return (
        <FormSection
            title="载入条件"
            description="写在 koishi.yml 里的表达式，条件不满足时即使开着也不载入；要改去原始文件"
        >
            <dl className="grid grid-cols-[auto_1fr] gap-x-5 gap-y-2 text-xs">
                {shown.map(([k, v]) => (
                    <div key={k} className="contents">
                        <dt className="font-mono text-text-tertiary">{k}</dt>
                        <dd className="break-all rounded bg-inset px-2 py-1 font-mono text-text-secondary">
                            {typeof v === 'string' ? v : JSON.stringify(v)}
                        </dd>
                    </div>
                ))}
            </dl>
        </FormSection>
    );
}

function GroupDetail(props: DetailProps) {
    const { config, path, node, onChange, disabled } = props;
    const label = typeof node.meta.$label === 'string' ? node.meta.$label : '';
    const plugins = walk(node.children).filter((n) => !isGroup(n));
    return (
        <div className="flex flex-col gap-7">
            <DetailHeader {...props} subtitle={`分组 · 里面有 ${plugins.length} 个插件`} />
            <FormSection title="分组">
                <div className="grid gap-x-5 sm:grid-cols-2">
                    <TextField
                        label="显示名"
                        value={label}
                        placeholder={node.ident}
                        hint="只影响树里怎么显示；留空显示标识"
                        disabled={disabled}
                        onValueChange={(v) =>
                            onChange(
                                replaceAt(config, path, (n) => {
                                    const meta = { ...n.meta };
                                    if (v.trim()) meta.$label = v;
                                    else delete meta.$label;
                                    return { ...n, meta };
                                }),
                            )
                        }
                    />
                </div>
            </FormSection>
            <MetaSection node={node} />
        </div>
    );
}

function PluginDetail(props: DetailProps) {
    const { instanceId, config, path, node, onChange, disabled } = props;
    const schema = useKoishiPluginSchema(instanceId, node.name);
    const [showUsage, setShowUsage] = useState(false);
    const [rawOpen, setRawOpen] = useState(false);
    const hydrated = hydrateSchema(schema.data?.schema);
    const formOk = !!hydrated && renderable(hydrated);
    const missing = hydrated && node.enabled ? missingRequired(hydrated, node.config) : [];
    const setConfig = (next: Record<string, unknown>) =>
        onChange(replaceAt(config, path, (n) => ({ ...n, config: next })));
    const subtitle = schema.data?.package
        ? `${schema.data.package}${schema.data.version ? ` · v${schema.data.version}` : ''}`
        : schema.isLoading
          ? '正在读取包信息…'
          : '包信息读不到';

    return (
        <div className="flex flex-col gap-7">
            <DetailHeader {...props} subtitle={subtitle} />

            {(isLinkNode(node) || missing.length > 0 || schema.data?.error) && (
                <div className="flex flex-col gap-2.5">
                    {isLinkNode(node) && (
                        <Notice title="这一条是桌面端对接时写的">
                            selfId 要和 Bot 的 QQ 号一致、路径要和 Bot 侧连接地址一致；换 Bot
                            请在页头重新对接
                        </Notice>
                    )}
                    {missing.length > 0 && (
                        <Notice tone="warning" title="还有必填项没填">
                            开着启动会报错：{missing.map((m) => m || '(整项)').join('、')}
                        </Notice>
                    )}
                    {schema.data?.error && (
                        <Notice tone="warning" title="读不到这个插件">
                            {schema.data.error}。包可能没装上，去插件市场装一下；配置可以先按原文改
                        </Notice>
                    )}
                </div>
            )}

            {schema.data?.usage && (
                <FormSection
                    title="使用说明"
                    actions={
                        <Button size="sm" variant="ghost" onClick={() => setShowUsage((v) => !v)}>
                            <BookOpen size={13} />
                            {showUsage ? '收起' : '展开'}
                        </Button>
                    }
                >
                    {showUsage ? (
                        <SimpleMarkdown
                            text={schema.data.usage}
                            className="text-[13px] leading-relaxed"
                        />
                    ) : (
                        <p className="line-clamp-2 text-xs leading-relaxed text-text-tertiary">
                            {schema.data.usage.replace(/[#>*`_-]/g, '').trim()}
                        </p>
                    )}
                </FormSection>
            )}

            {schema.isLoading ? (
                <PaneLoading text="正在读取插件的表单…" />
            ) : formOk ? (
                <div className="flex flex-col gap-4">
                    <div className="flex items-center justify-end">
                        <ModeSwitch raw={rawOpen} onChange={setRawOpen} />
                    </div>
                    {rawOpen ? (
                        <RawConfigEditor
                            schema={hydrated}
                            value={node.config}
                            onChange={setConfig}
                            disabled={disabled}
                        />
                    ) : (
                        <SchemasteryForm
                            schema={hydrated}
                            value={node.config}
                            onChange={setConfig}
                            disabled={disabled}
                        />
                    )}
                </div>
            ) : (
                <FallbackConfig
                    broken={!!schema.data?.error}
                    value={node.config}
                    onChange={setConfig}
                    disabled={disabled}
                />
            )}

            <MetaSection node={node} />
        </div>
    );
}

/** 表单 / 原文 JSON 切换，样式和日志级别筛选的段控件一致 */
function ModeSwitch({ raw, onChange }: { raw: boolean; onChange: (raw: boolean) => void }) {
    return (
        <div
            role="tablist"
            aria-label="配置编辑方式"
            className="flex h-7 items-center gap-0.5 rounded-md bg-inset/60 p-0.5"
        >
            {[
                { v: false, label: '表单' },
                { v: true, label: '原文 JSON' },
            ].map((m) => (
                <button
                    key={m.label}
                    type="button"
                    role="tab"
                    aria-selected={raw === m.v}
                    onClick={() => onChange(m.v)}
                    className={cn(
                        'h-6 rounded-sm px-2.5 text-[11.5px] font-medium leading-6 transition-colors',
                        raw === m.v
                            ? 'bg-surface text-text shadow-[0_1px_2px_rgba(0,0,0,0.04)]'
                            : 'text-text-tertiary hover:text-text',
                    )}
                >
                    {m.label}
                </button>
            ))}
        </div>
    );
}

/** 插件没声明配置表单时：空配置不给一堆看不懂的空编辑框，确实要写再展开 */
function FallbackConfig({
    broken,
    value,
    onChange,
    disabled,
}: {
    /** 包读不到（可能没装上）：原文是唯一的改法，直接摊开 */
    broken: boolean;
    value: Record<string, unknown>;
    onChange: (next: Record<string, unknown>) => void;
    disabled?: boolean;
}) {
    const [forceOpen, setForceOpen] = useState(false);
    const empty = Object.keys(value).length === 0;
    if (empty && !broken && !forceOpen) {
        return (
            <FormSection title="配置">
                <div className="flex flex-wrap items-center justify-between gap-3 rounded-md border border-dashed border-border-subtle px-4 py-3.5">
                    <p className="text-xs leading-relaxed text-text-tertiary">
                        这个插件没有声明配置项，保持空配置就行；少数插件仍接受自定义键
                    </p>
                    <Button size="sm" variant="ghost" onClick={() => setForceOpen(true)}>
                        按原文 JSON 写
                    </Button>
                </div>
            </FormSection>
        );
    }
    return <RawConfigEditor schema={null} value={value} onChange={onChange} disabled={disabled} />;
}

/**
 * 原文 JSON：CodeMirror（着色、括号配对、语法诊断，schema 在时给键名 / 枚举补全）。
 * 空配置打开时按 schema 默认值预填，写回时把和默认值一样的键删掉，koishi.yml 保持干净。
 */
function RawConfigEditor({
    schema,
    value,
    onChange,
    disabled,
}: {
    schema: SNode | null;
    value: Record<string, unknown>;
    onChange: (next: Record<string, unknown>) => void;
    disabled?: boolean;
}) {
    const [text, setText] = useState(() => {
        const base =
            Object.keys(value).length > 0
                ? value
                : schema
                  ? materializeConfig(schema, value)
                  : value;
        return JSON.stringify(base, null, 2);
    });
    const [error, setError] = useState<string | null>(null);
    const jsonSchema = useMemo(
        () => (schema ? toJsonSchema(schema, value) : null),
        // 只在进原文模式时取一次；值本身边改边进不了补全，不影响
        // eslint-disable-next-line react-hooks/exhaustive-deps
        [schema],
    );
    const commit = (raw: string) => {
        try {
            const parsed = raw.trim() ? JSON.parse(raw) : {};
            if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed))
                throw new Error('要是一个对象');
            onChange(
                schema
                    ? simplifyConfig(schema, parsed as Record<string, unknown>)
                    : (parsed as Record<string, unknown>),
            );
            setError(null);
        } catch (e) {
            setError(`不是合法的 JSON 对象：${(e as Error).message}`);
        }
    };
    return (
        <FormSection
            title="配置原文"
            description="就是 koishi.yml 里这一项的内容；没写的键按默认值生效，写回时和默认值一样的键不落下"
            actions={
                <Button
                    size="sm"
                    variant="secondary"
                    disabled={disabled}
                    onClick={() => commit(text)}
                >
                    写回配置
                </Button>
            }
        >
            <div
                onBlur={(e) => {
                    if (!e.currentTarget.contains(e.relatedTarget as Node | null)) commit(text);
                }}
            >
                <JsonCodeEditor
                    className="h-64 flex-none"
                    value={text}
                    onChange={setText}
                    schema={jsonSchema}
                    readOnly={disabled}
                    onSubmit={() => commit(text)}
                    ariaLabel="插件配置原文 JSON"
                />
            </div>
            <p className={cn('mt-2 text-2xs', error ? 'text-danger' : 'text-text-tertiary')}>
                {error ?? '失焦或 Ctrl+Enter 写回；语法错误会在行号旁边标出来'}
            </p>
        </FormSection>
    );
}

export function KoishiNodeDetail(props: DetailProps) {
    return isGroup(props.node) ? (
        <GroupDetail {...props} />
    ) : (
        <PluginDetail key={nodeKey(props.node)} {...props} />
    );
}
