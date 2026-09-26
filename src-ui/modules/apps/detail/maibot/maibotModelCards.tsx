// 模型页的三种卡片：提供商、模型、任务。model_config 这份 schema 上游没写中文标签、下拉也没给选项，
// 这里照 model_configs.py 里的说明手写，字段和生成的类型一一对应。

import { useRef, type ReactNode } from 'react';
import { Plus, Trash2, X } from 'lucide-react';
import { NumberField, Select, Switch, TextAreaField, TextField, type SelectItem } from '../../../../shared/ui';
import { cn } from '../../../../shared/utils/cn';
import { MAIBOT_PLACEHOLDER_API_KEY } from '../../../../core/domain/apps/maibotConfig';
import type { MaiBotAPIProvider, MaiBotModelInfo, MaiBotTaskConfig } from '../../../../core/ipc/types';
import { CONFIG_PAIR } from '../karin/configLayout';
import { StringMapEditor } from './listEditors';
import { WIDE } from './SchemaForm';

type Errors = Record<string, string>;

/** 文件里写了列表外的值（插件扩展的接口类型之类）也得选得中，不然一渲染就像被清掉了 */
function withCurrent(items: readonly SelectItem[], value: string): SelectItem[] {
    return !value || items.some((i) => i.value === value) ? [...items] : [...items, { value, label: value }];
}

const CLIENT_TYPES: SelectItem[] = [
    { value: 'openai', label: 'OpenAI 兼容' },
    { value: 'openai_responses', label: 'OpenAI Responses' },
    { value: 'gemini', label: 'Gemini' },
];
const AUTH_TYPES: SelectItem[] = [
    { value: 'bearer', label: 'Bearer 令牌' },
    { value: 'header', label: '自定义请求头' },
    { value: 'query', label: '查询参数' },
    { value: 'none', label: '不鉴权' },
];
const REASONING_MODES: SelectItem[] = [
    { value: 'auto', label: '自动' },
    { value: 'native', label: '接口原生字段' },
    { value: 'think_tag', label: '<think> 标签' },
    { value: 'none', label: '不解析' },
];
const TOOL_ARG_MODES: SelectItem[] = [
    { value: 'auto', label: '自动' },
    { value: 'strict', label: '严格' },
    { value: 'repair', label: '尽量修复' },
    { value: 'double_decode', label: '二次解码' },
];
const STRATEGIES: SelectItem[] = [
    { value: 'balance', label: '负载均衡' },
    { value: 'random', label: '随机' },
    { value: 'sequential', label: '按顺序，前面的不行再换' },
];

const Card: React.FC<{
    title: ReactNode;
    meta?: ReactNode;
    onRemove?: () => void;
    disabled?: boolean;
    children: ReactNode;
}> = ({ title, meta, onRemove, disabled, children }) => (
    <div className="flex flex-col gap-4 rounded-md border border-border-subtle bg-inset/30 p-4">
        <div className="flex min-w-0 items-center gap-2">
            <span className="min-w-0 truncate text-sm font-medium text-text">{title}</span>
            {meta}
            <span className="flex-1" />
            {onRemove && (
                <button
                    type="button"
                    aria-label="删掉"
                    disabled={disabled}
                    onClick={onRemove}
                    className="inline-flex h-7 w-7 items-center justify-center rounded-xs text-text-tertiary hover:bg-danger-soft hover:text-danger disabled:opacity-40"
                >
                    <Trash2 size={14} strokeWidth={2.2} />
                </button>
            )}
        </div>
        {children}
    </div>
);

/**
 * 被别处按名字引用的名字。打字时只改自己，失焦时才把引用跟过去：
 * 边打边改的话，打到一半和别的重名就会把人家的引用也并过来。
 */
const NameField: React.FC<{
    label: string;
    hint?: string;
    value: string;
    error?: string;
    placeholder?: string;
    disabled?: boolean;
    onChange: (next: string) => void;
    onCommit: (from: string, to: string) => void;
}> = ({ label, hint, value, error, placeholder, disabled, onChange, onCommit }) => {
    const start = useRef<string | null>(null);
    return (
        <TextField
            label={label}
            hint={hint}
            value={value}
            error={error}
            placeholder={placeholder}
            disabled={disabled}
            onValueChange={onChange}
            onFocus={() => {
                start.current = value;
            }}
            onBlur={() => {
                const from = start.current;
                start.current = null;
                if (from !== null && from !== value) onCommit(from, value);
            }}
        />
    );
};

// 必填的数字清空时保留原值；能留空的（模型的温度、最大输出）单独按 undefined 处理
const numOrKeep = (v: number | null, keep: number) => (v === null ? keep : v);

export const ProviderCard: React.FC<{
    provider: MaiBotAPIProvider;
    /** 错误路径前缀，如 models/api_providers/0 */
    path: string;
    errors: Errors;
    disabled?: boolean;
    showAdvanced: boolean;
    onChange: (next: MaiBotAPIProvider) => void;
    onRename: (from: string, to: string) => void;
    onRemove: () => void;
}> = ({ provider: p, path, errors, disabled, showAdvanced, onChange, onRename, onRemove }) => {
    const set = (patch: Partial<MaiBotAPIProvider>) => onChange({ ...p, ...patch });
    const err = (k: string) => errors[`${path}/${k}`];
    const typeLabel = CLIENT_TYPES.find((t) => t.value === p.client_type)?.label ?? p.client_type;
    const placeholderKey = p.api_key.trim() === MAIBOT_PLACEHOLDER_API_KEY;
    return (
        <Card
            title={p.name || '未命名提供商'}
            meta={<span className="shrink-0 text-2xs text-text-tertiary">{typeLabel}</span>}
            onRemove={onRemove}
            disabled={disabled}
        >
            <div className={CONFIG_PAIR}>
                <NameField
                    label="名称"
                    hint="模型里按这个名字选提供商"
                    value={p.name}
                    error={err('name')}
                    placeholder="DeepSeek"
                    disabled={disabled}
                    onChange={(name) => set({ name })}
                    onCommit={onRename}
                />
                <Select
                    label="接口类型"
                    items={withCurrent(CLIENT_TYPES, p.client_type)}
                    value={p.client_type}
                    disabled={disabled}
                    onValueChange={(client_type) => set({ client_type })}
                />
                <TextField
                    className={WIDE}
                    label="接口地址"
                    value={p.base_url}
                    placeholder="https://api.example.com/v1"
                    hint={p.client_type === 'gemini' ? 'Gemini 可以留空' : undefined}
                    error={err('base_url')}
                    disabled={disabled}
                    onValueChange={(base_url) => set({ base_url })}
                />
                <TextField
                    className={WIDE}
                    label="API Key"
                    type="password"
                    autoComplete="off"
                    value={p.api_key}
                    placeholder={p.auth_type === 'none' ? '不鉴权，可以留空' : 'sk-...'}
                    hint={placeholderKey ? '这是示例里的占位 Key，换成你自己的才能用' : undefined}
                    error={err('api_key')}
                    disabled={disabled}
                    onValueChange={(api_key) => set({ api_key })}
                />
            </div>
            {showAdvanced && (
                <div className={CONFIG_PAIR}>
                    <Select
                        label="鉴权方式"
                        items={withCurrent(AUTH_TYPES, p.auth_type)}
                        value={p.auth_type}
                        disabled={disabled}
                        onValueChange={(auth_type) => set({ auth_type })}
                    />
                    <TextField
                        label="模型列表路径"
                        value={p.model_list_endpoint}
                        disabled={disabled}
                        onValueChange={(model_list_endpoint) => set({ model_list_endpoint })}
                    />
                    {p.auth_type === 'header' && (
                        <>
                            <TextField
                                label="请求头名"
                                value={p.auth_header_name}
                                error={err('auth_header_name')}
                                disabled={disabled}
                                onValueChange={(auth_header_name) => set({ auth_header_name })}
                            />
                            <TextField
                                label="请求头前缀"
                                hint="留空就直接发 Key"
                                value={p.auth_header_prefix}
                                disabled={disabled}
                                onValueChange={(auth_header_prefix) => set({ auth_header_prefix })}
                            />
                        </>
                    )}
                    {p.auth_type === 'query' && (
                        <TextField
                            label="查询参数名"
                            value={p.auth_query_name}
                            error={err('auth_query_name')}
                            disabled={disabled}
                            onValueChange={(auth_query_name) => set({ auth_query_name })}
                        />
                    )}
                    <Select
                        label="推理内容解析"
                        items={withCurrent(REASONING_MODES, p.reasoning_parse_mode)}
                        value={p.reasoning_parse_mode}
                        disabled={disabled}
                        onValueChange={(reasoning_parse_mode) => set({ reasoning_parse_mode })}
                    />
                    <Select
                        label="工具参数解析"
                        items={withCurrent(TOOL_ARG_MODES, p.tool_argument_parse_mode)}
                        value={p.tool_argument_parse_mode}
                        disabled={disabled}
                        onValueChange={(tool_argument_parse_mode) => set({ tool_argument_parse_mode })}
                    />
                    <NumberField
                        label="失败重试次数"
                        min={0}
                        value={p.max_retry}
                        error={err('max_retry')}
                        disabled={disabled}
                        onValueChange={(v) => set({ max_retry: numOrKeep(v, p.max_retry) })}
                    />
                    <NumberField
                        label="超时（秒）"
                        min={1}
                        value={p.timeout}
                        error={err('timeout')}
                        disabled={disabled}
                        onValueChange={(v) => set({ timeout: numOrKeep(v, p.timeout) })}
                    />
                    <NumberField
                        label="重试间隔（秒）"
                        min={1}
                        value={p.retry_interval}
                        error={err('retry_interval')}
                        disabled={disabled}
                        onValueChange={(v) => set({ retry_interval: numOrKeep(v, p.retry_interval) })}
                    />
                    {p.client_type !== 'gemini' && (
                        <>
                            <TextField
                                label="Organization"
                                hint="OpenAI 官方接口可选"
                                value={p.organization ?? ''}
                                disabled={disabled}
                                onValueChange={(v) => set({ organization: v || undefined })}
                            />
                            <TextField
                                label="Project"
                                hint="OpenAI 官方接口可选"
                                value={p.project ?? ''}
                                disabled={disabled}
                                onValueChange={(v) => set({ project: v || undefined })}
                            />
                        </>
                    )}
                    <StringMapEditor
                        className={WIDE}
                        label="默认请求头"
                        keyPlaceholder="HTTP-Referer"
                        value={p.default_headers}
                        disabled={disabled}
                        onChange={(default_headers) => set({ default_headers })}
                    />
                    <StringMapEditor
                        className={WIDE}
                        label="默认查询参数"
                        value={p.default_query}
                        disabled={disabled}
                        onChange={(default_query) => set({ default_query })}
                    />
                </div>
            )}
        </Card>
    );
};

export const ModelCard: React.FC<{
    model: MaiBotModelInfo;
    path: string;
    errors: Errors;
    providers: readonly string[];
    disabled?: boolean;
    showAdvanced: boolean;
    onChange: (next: MaiBotModelInfo) => void;
    onRename: (from: string, to: string) => void;
    onRemove: () => void;
}> = ({ model: m, path, errors, providers, disabled, showAdvanced, onChange, onRename, onRemove }) => {
    const set = (patch: Partial<MaiBotModelInfo>) => onChange({ ...m, ...patch });
    const err = (k: string) => errors[`${path}/${k}`];
    const providerItems = withCurrent(
        providers.map((n) => ({ value: n, label: n })),
        m.api_provider,
    );
    return (
        <Card
            title={m.name || '未命名模型'}
            meta={
                <span className="shrink-0 truncate font-mono text-2xs text-text-tertiary">
                    {[m.api_provider, m.model_identifier].filter(Boolean).join(' / ')}
                </span>
            }
            onRemove={onRemove}
            disabled={disabled}
        >
            <div className={CONFIG_PAIR}>
                <NameField
                    label="名称"
                    hint="任务里按这个名字挑模型"
                    value={m.name}
                    error={err('name')}
                    disabled={disabled}
                    onChange={(name) => set({ name })}
                    onCommit={onRename}
                />
                <TextField
                    label="模型标识"
                    hint="服务商那边的模型 ID"
                    value={m.model_identifier}
                    placeholder="deepseek-chat"
                    error={err('model_identifier')}
                    disabled={disabled}
                    onValueChange={(model_identifier) => set({ model_identifier })}
                />
                <Select
                    label="提供商"
                    placeholder="选一个提供商"
                    items={providerItems}
                    value={m.api_provider || undefined}
                    error={err('api_provider')}
                    disabled={disabled}
                    onValueChange={(api_provider) => set({ api_provider })}
                />
                <Switch
                    label="能看图"
                    hint="开了才会被派去识图"
                    checked={m.visual}
                    disabled={disabled}
                    onCheckedChange={(visual) => set({ visual })}
                />
                <NumberField
                    label="温度"
                    allowFloat
                    min={0}
                    max={2}
                    step={0.1}
                    value={m.temperature ?? null}
                    hint="留空用任务里的温度"
                    error={err('temperature')}
                    disabled={disabled}
                    onValueChange={(v) => set({ temperature: v ?? undefined })}
                />
                <NumberField
                    label="最大输出 token"
                    min={1}
                    value={m.max_tokens ?? null}
                    hint="留空用任务里的设置"
                    error={err('max_tokens')}
                    disabled={disabled}
                    onValueChange={(v) => set({ max_tokens: v ?? undefined })}
                />
            </div>
            {showAdvanced && (
                <div className={CONFIG_PAIR}>
                    <NumberField
                        label="输入价格"
                        hint="元 / 百万 token，只用来统计花费"
                        allowFloat
                        min={0}
                        value={m.price_in}
                        disabled={disabled}
                        onValueChange={(v) => set({ price_in: numOrKeep(v, m.price_in) })}
                    />
                    <NumberField
                        label="输出价格"
                        hint="元 / 百万 token"
                        allowFloat
                        min={0}
                        value={m.price_out}
                        disabled={disabled}
                        onValueChange={(v) => set({ price_out: numOrKeep(v, m.price_out) })}
                    />
                    <Switch
                        label="缓存计费"
                        hint="命中缓存的输入按缓存价算"
                        checked={m.cache}
                        disabled={disabled}
                        onCheckedChange={(cache) => set({ cache })}
                    />
                    {m.cache ? (
                        <NumberField
                            label="缓存命中价格"
                            hint="元 / 百万 token"
                            allowFloat
                            min={0}
                            value={m.cache_price_in}
                            disabled={disabled}
                            onValueChange={(v) => set({ cache_price_in: numOrKeep(v, m.cache_price_in) })}
                        />
                    ) : (
                        <span className="hidden sm:block" />
                    )}
                    <Switch
                        label="发送温度参数"
                        hint="有的模型不收 temperature，关掉就不发"
                        checked={m.send_temperature}
                        disabled={disabled}
                        onCheckedChange={(send_temperature) => set({ send_temperature })}
                    />
                    <Switch
                        label="强制流式输出"
                        hint="模型只支持流式时打开"
                        checked={m.force_stream_mode}
                        disabled={disabled}
                        onCheckedChange={(force_stream_mode) => set({ force_stream_mode })}
                    />
                    <TextAreaField
                        className={WIDE}
                        mono
                        minRows={2}
                        label="额外参数"
                        hint='行内 TOML 表，如 { thinking = { type = "disabled" } }；headers / query 分别并进请求头、查询参数，其余进请求体'
                        value={m.extra_params}
                        error={err('extra_params')}
                        disabled={disabled}
                        onValueChange={(extra_params) => set({ extra_params })}
                    />
                </div>
            )}
        </Card>
    );
};

/** 任务挑模型：挑中的按顺序排（「按顺序」策略就是这个顺序），没挑的列在下面点一下加上 */
const ModelPicker: React.FC<{
    value: string[];
    models: readonly MaiBotModelInfo[];
    errorAt: (index: number) => string | undefined;
    disabled?: boolean;
    onChange: (next: string[]) => void;
}> = ({ value, models, errorAt, disabled, onChange }) => {
    const rest = models.map((m) => m.name).filter((n) => n && !value.includes(n));
    const firstError = value.map((_, i) => errorAt(i)).find(Boolean);
    return (
        <div className={cn('flex flex-col gap-2', WIDE)}>
            <span className="text-xs font-medium text-text-secondary">模型</span>
            <div className="flex flex-wrap gap-1.5">
                {value.length === 0 && <span className="text-2xs text-text-tertiary">还没挑</span>}
                {value.map((name, i) => (
                    <span
                        key={`${name}-${i}`}
                        className={cn(
                            'inline-flex h-7 items-center gap-1.5 rounded-full border pl-2.5 pr-1 font-mono text-xs',
                            errorAt(i) ? 'border-danger/50 bg-danger-soft/40 text-danger' : 'border-brand/30 bg-brand/10 text-text',
                        )}
                    >
                        <span className="text-2xs text-text-tertiary">{i + 1}</span>
                        {name}
                        <button
                            type="button"
                            aria-label={`去掉 ${name}`}
                            disabled={disabled}
                            onClick={() => onChange(value.filter((_, j) => j !== i))}
                            className="inline-flex h-5 w-5 items-center justify-center rounded-full hover:bg-inset disabled:opacity-40"
                        >
                            <X size={12} strokeWidth={2.4} />
                        </button>
                    </span>
                ))}
            </div>
            {rest.length > 0 && (
                <div className="flex flex-wrap gap-1.5">
                    {rest.map((name) => (
                        <button
                            key={name}
                            type="button"
                            disabled={disabled}
                            onClick={() => onChange([...value, name])}
                            className="inline-flex h-7 items-center gap-1 rounded-full border border-dashed border-border-subtle px-2.5 font-mono text-xs text-text-secondary hover:border-brand/40 hover:text-text disabled:opacity-40"
                        >
                            <Plus size={12} strokeWidth={2.4} />
                            {name}
                        </button>
                    ))}
                </div>
            )}
            {firstError && <p className="text-2xs text-danger">{firstError}</p>}
        </div>
    );
};

export const TaskCard: React.FC<{
    title: string;
    hint: string;
    required?: boolean;
    task: MaiBotTaskConfig;
    path: string;
    errors: Errors;
    models: readonly MaiBotModelInfo[];
    disabled?: boolean;
    showAdvanced: boolean;
    onChange: (next: MaiBotTaskConfig) => void;
}> = ({ title, hint, required, task: t, path, errors, models, disabled, showAdvanced, onChange }) => {
    const set = (patch: Partial<MaiBotTaskConfig>) => onChange({ ...t, ...patch });
    const err = (k: string) => errors[`${path}/${k}`];
    return (
        <Card
            title={title}
            meta={
                <span className={cn('shrink-0 text-2xs', required && !t.model_list.length ? 'text-warning' : 'text-text-tertiary')}>
                    {required ? '必须挑' : hint}
                </span>
            }
        >
            {required && <p className="-mt-2 text-2xs text-text-tertiary">{hint}</p>}
            <div className={CONFIG_PAIR}>
                <ModelPicker
                    value={t.model_list}
                    models={models}
                    errorAt={(i) => errors[`${path}/model_list/${i}`]}
                    disabled={disabled}
                    onChange={(model_list) => set({ model_list })}
                />
                <NumberField
                    label="温度"
                    allowFloat
                    min={0}
                    max={2}
                    step={0.1}
                    value={t.temperature}
                    error={err('temperature')}
                    disabled={disabled}
                    onValueChange={(v) => set({ temperature: numOrKeep(v, t.temperature) })}
                />
                <NumberField
                    label="最大输出 token"
                    min={1}
                    value={t.max_tokens}
                    error={err('max_tokens')}
                    disabled={disabled}
                    onValueChange={(v) => set({ max_tokens: numOrKeep(v, t.max_tokens) })}
                />
                {t.model_list.length > 1 && (
                    <Select
                        className={WIDE}
                        label="挑了多个时怎么选"
                        items={withCurrent(STRATEGIES, t.selection_strategy)}
                        value={t.selection_strategy}
                        disabled={disabled}
                        onValueChange={(selection_strategy) => set({ selection_strategy })}
                    />
                )}
                {showAdvanced && (
                    <>
                        <NumberField
                            label="慢请求警告（秒）"
                            hint="超过这么久在日志里提醒"
                            allowFloat
                            min={0}
                            value={t.slow_threshold}
                            error={err('slow_threshold')}
                            disabled={disabled}
                            onValueChange={(v) => set({ slow_threshold: numOrKeep(v, t.slow_threshold) })}
                        />
                        <NumberField
                            label="硬超时（秒）"
                            hint="到点没回就取消，换下一个模型"
                            allowFloat
                            min={1}
                            value={t.hard_timeout}
                            error={err('hard_timeout')}
                            disabled={disabled}
                            onValueChange={(v) => set({ hard_timeout: numOrKeep(v, t.hard_timeout) })}
                        />
                    </>
                )}
            </div>
        </Card>
    );
};
