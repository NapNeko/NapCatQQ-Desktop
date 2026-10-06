// 知识库「导入」：粘一段文字或挑本机文件，说清是什么内容、给哪个聊天用；下面是导入记录，
// 有在跑的就自动刷进度，跑完了让浏览、图谱跟着刷新。

import { useEffect, useRef, useState, type ReactNode } from 'react';
import { ClipboardPaste, FileText, RotateCcw, Upload, X } from 'lucide-react';
import {
    Badge,
    Button,
    Progress,
    Select,
    Switch,
    TextAreaField,
    TextField,
} from '../../../../shared/ui';
import { cn } from '../../../../shared/utils/cn';
import type {
    MaiBotLocalTextFile,
    MaiBotMemoryImportKind,
    MaiBotMemoryImportOptions,
    MaiBotMemoryTask,
    MaiBotMemoryTaskStatus,
} from '../../../../core/ipc/types';
import {
    taskActive,
    useMaiBotMemoryImport,
    useMaiBotMemoryImportFiles,
    useMaiBotMemoryImportSetup,
    useMaiBotMemoryTask,
    useMaiBotMemoryTaskAction,
    useMaiBotMemoryTasks,
    useRefreshMemoryData,
} from '../../../../hooks/apps/useMaiBotMemory';
import { pushInfoBar } from '../../../../hooks/ui/globalInfoBarStore';
import { useTauriFileDrop } from '../../../../hooks/ui/useTauriFileDrop';
import { ResourcePane, Segmented } from '../resourceParts';
import { relativeTime } from './maibotPromptParts';

const KINDS: { value: MaiBotMemoryImportKind; label: string; hint: string }[] = [
    { value: 'auto', label: '自动判断', hint: '按内容自己判断怎么切' },
    { value: 'narrative', label: '故事、经历', hint: '连着读才有意思的，按段落切' },
    { value: 'factual', label: '设定、资料', hint: '一条一条的，按条切' },
    { value: 'quote', label: '语录、原话', hint: '原样记下，不改写' },
    { value: 'chat_log', label: '聊天记录', hint: '会认出谁在什么时候说了什么' },
];
const GLOBAL = '__global__';

export const STATUS_TEXT: Readonly<Record<MaiBotMemoryTaskStatus, string>> = {
    queued: '排队中',
    preparing: '准备中',
    running: '导入中',
    cancelling: '正在取消',
    done: '导完了',
    done_with_errors: '有没成的',
    cancelled: '取消了',
    failed: '失败',
};

const STATUS_TONE: Readonly<
    Record<MaiBotMemoryTaskStatus, 'neutral' | 'info' | 'success' | 'warning' | 'danger'>
> = {
    queued: 'neutral',
    preparing: 'info',
    running: 'info',
    cancelling: 'neutral',
    done: 'success',
    done_with_errors: 'warning',
    cancelled: 'neutral',
    failed: 'danger',
};

function formatSize(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KB`;
    return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

const firstLine = (text: string) => {
    const line = text.trim().split('\n')[0] ?? '';
    return line.length > 24 ? `${line.slice(0, 24)}…` : line;
};

export const KnowledgeImport: React.FC<{ instanceId: string; switcher: ReactNode }> = ({
    instanceId,
    switcher,
}) => {
    const setup = useMaiBotMemoryImportSetup(instanceId, true);
    const limits = setup.data?.limits;
    const tasks = useMaiBotMemoryTasks(instanceId, true, limits?.poll_ms);
    const run = useMaiBotMemoryImport(instanceId);
    const refresh = useRefreshMemoryData(instanceId);

    const [mode, setMode] = useState<'paste' | 'files'>('paste');
    const [name, setName] = useState('');
    const [content, setContent] = useState('');
    const [files, setFiles] = useState<MaiBotLocalTextFile[]>([]);
    const [options, setOptions] = useState<MaiBotMemoryImportOptions>({
        kind: 'auto',
        chat_id: '',
        use_llm: true,
        force: false,
    });
    // 这次会话里建的任务记着导的是什么；上游的任务列表里没有文件名
    const [labels, setLabels] = useState<Record<string, string>>({});
    const [openTask, setOpenTask] = useState<string | null>(null);

    // 跑完一个任务，记忆就变了：浏览、图谱刷新，顺便说一声
    const running = useRef(new Set<string>());
    useEffect(() => {
        for (const t of tasks.data ?? []) {
            if (taskActive(t)) running.current.add(t.id);
            else if (running.current.delete(t.id)) {
                refresh();
                if (t.status === 'done' || t.status === 'done_with_errors') {
                    pushInfoBar({
                        key: `maibotMemoryTaskDone:${t.id}`,
                        tone: t.status === 'done' ? 'success' : 'warning',
                        title: t.status === 'done' ? '导完了' : `导完了，${t.failed_chunks} 块没成`,
                        content: labels[t.id],
                        autoDismissMs: 4000,
                    });
                }
            }
        }
    }, [tasks.data, refresh, labels]);

    const importFiles = useMaiBotMemoryImportFiles();
    const addFiles = async (paths: string[]) => {
        if (paths.length === 0) return;
        const seen = await importFiles.read(paths);
        setFiles((prev) => [...prev, ...seen.filter((f) => !prev.some((p) => p.path === f.path))]);
    };
    const { dragging } = useTauriFileDrop(true, (paths) => {
        setMode('files');
        void addFiles(paths);
    });

    const okFiles = files.filter((f) => !f.problem);
    const tooLong = !!limits && content.length > limits.max_paste_chars;
    const canRun = mode === 'paste' ? content.trim().length > 0 && !tooLong : okFiles.length > 0;
    const kind = KINDS.find((k) => k.value === options.kind) ?? KINDS[0];
    const set = (patch: Partial<MaiBotMemoryImportOptions>) =>
        setOptions((o) => ({ ...o, ...patch }));

    const submit = () => {
        const label =
            mode === 'paste'
                ? name.trim() || firstLine(content)
                : okFiles.map((f) => f.name).join('、');
        const req =
            mode === 'paste'
                ? ({ op: 'paste', name, content, options } as const)
                : ({ op: 'files', paths: okFiles.map((f) => f.path), options } as const);
        void run.mutateAsync(req).then((task) => {
            setLabels((prev) => ({ ...prev, [task.id]: label }));
            if (mode === 'paste') {
                setContent('');
                setName('');
            } else setFiles([]);
        });
    };

    const list = tasks.data ?? [];
    return (
        <ResourcePane toolbar={switcher}>
            {/* 宽屏左右排：点了导入马上能在右边看到进度，不用往下翻 */}
            <div className="grid items-start gap-4 lg:grid-cols-[minmax(0,1fr)_20rem]">
                <section className="relative flex flex-col gap-3 rounded-md border border-border-subtle bg-surface p-4">
                    <div className="flex flex-wrap items-center justify-between gap-2">
                        <Segmented
                            items={[
                                { value: 'paste', label: '粘贴文字' },
                                {
                                    value: 'files',
                                    label: '选文件',
                                    count: files.length || undefined,
                                },
                            ]}
                            value={mode}
                            onChange={setMode}
                        />
                        <span className="text-2xs text-text-tertiary">
                            导进去的成为麦麦的长期记忆，聊到了会想起来
                        </span>
                    </div>
                    {mode === 'paste' ? (
                        <>
                            <TextField
                                label="名字"
                                placeholder="比如：麦麦的设定。以后能按它找到、整批删掉"
                                value={name}
                                onValueChange={setName}
                            />
                            <TextAreaField
                                label="内容"
                                minRows={6}
                                placeholder="把资料、设定、聊天记录粘在这里"
                                value={content}
                                onValueChange={setContent}
                                error={
                                    tooLong
                                        ? `太长了，一次最多 ${limits?.max_paste_chars.toLocaleString()} 字`
                                        : undefined
                                }
                            />
                            <span className="-mt-2 self-end font-mono text-2xs text-text-tertiary">
                                {content.length.toLocaleString()}
                                {limits && ` / ${limits.max_paste_chars.toLocaleString()}`}
                            </span>
                        </>
                    ) : (
                        <FilePicker
                            files={files}
                            limits={limits}
                            onPick={() => void importFiles.pick().then(addFiles)}
                            onRemove={(p) => setFiles((fs) => fs.filter((f) => f.path !== p))}
                        />
                    )}
                    <div className="grid gap-3 md:grid-cols-2">
                        <div className="flex flex-col gap-1">
                            <Select
                                label="内容是"
                                items={KINDS.map((k) => ({ value: k.value, label: k.label }))}
                                value={options.kind}
                                onValueChange={(v) => set({ kind: v as MaiBotMemoryImportKind })}
                            />
                            <span className="text-2xs text-text-tertiary">{kind.hint}</span>
                        </div>
                        <div className="flex flex-col gap-1">
                            <Select
                                label="给谁用"
                                items={[
                                    { value: GLOBAL, label: '所有聊天' },
                                    ...(setup.data?.chats ?? []).map((c) => ({
                                        value: c.chat_id,
                                        label: c.chat_name,
                                    })),
                                ]}
                                value={options.chat_id || GLOBAL}
                                onValueChange={(v) => set({ chat_id: v === GLOBAL ? '' : v })}
                            />
                            <span className="text-2xs text-text-tertiary">
                                {options.chat_id
                                    ? '只在这个聊天里想得起来'
                                    : '哪个聊天里都想得起来'}
                            </span>
                        </div>
                    </div>
                    <div className="flex flex-wrap items-end justify-between gap-3">
                        <div className="flex flex-col gap-2">
                            <Switch
                                label="用模型抽取人物和关系"
                                hint="图谱靠它长出来；更准，但慢、要花 token"
                                checked={options.use_llm}
                                onCheckedChange={(use_llm) => set({ use_llm })}
                            />
                            <Switch
                                label="内容重复也再导一遍"
                                hint="默认和导过的一模一样会跳过"
                                checked={options.force}
                                onCheckedChange={(force) => set({ force })}
                            />
                        </div>
                        <Button
                            variant="primary"
                            disabled={!canRun || run.isPending}
                            onClick={submit}
                        >
                            <Upload size={14} />
                            {mode === 'files' && okFiles.length > 0
                                ? `导入 ${okFiles.length} 个文件`
                                : '导入'}
                        </Button>
                    </div>
                    {dragging && (
                        <div className="pointer-events-none absolute inset-0 z-10 flex items-center justify-center rounded-md border-2 border-dashed border-brand/60 bg-brand-soft/70">
                            <span className="text-sm font-medium text-brand">松手就加进来</span>
                        </div>
                    )}
                </section>

                <section className="flex flex-col gap-2 lg:sticky lg:top-0">
                    <h3 className="flex flex-col gap-0.5 text-xs font-medium text-text-secondary">
                        导入记录
                        <span className="font-normal text-2xs text-text-tertiary">
                            麦麦重启后记录会清空，导进去的记忆不会
                        </span>
                    </h3>
                    {list.length === 0 ? (
                        <p className="rounded-md border border-dashed border-border-subtle px-4 py-6 text-center text-xs text-text-tertiary">
                            还没导过。导一次，这里能看到进度。
                        </p>
                    ) : (
                        list.map((t) => (
                            <TaskRow
                                key={t.id}
                                instanceId={instanceId}
                                task={t}
                                label={labels[t.id]}
                                open={openTask === t.id}
                                pollMs={limits?.poll_ms}
                                onToggle={() => setOpenTask((cur) => (cur === t.id ? null : t.id))}
                            />
                        ))
                    )}
                </section>
            </div>
        </ResourcePane>
    );
};

const FilePicker: React.FC<{
    files: readonly MaiBotLocalTextFile[];
    limits: { max_file_mb: number } | undefined;
    onPick: () => void;
    onRemove: (path: string) => void;
}> = ({ files, limits, onPick, onRemove }) => (
    <div className="flex flex-col gap-2">
        <button
            type="button"
            onClick={onPick}
            className="flex flex-col items-center justify-center gap-1.5 rounded-md border-2 border-dashed border-border px-4 py-6 text-text-tertiary transition-colors hover:border-brand/50 hover:text-brand"
        >
            <FileText size={20} />
            <span className="text-sm">挑文件，或者直接拖进窗口</span>
            <span className="text-2xs">
                txt / md / json，每个 {limits?.max_file_mb ?? 20} MB 以内，UTF-8 编码
            </span>
        </button>
        {files.length > 0 && (
            <ul className="flex flex-col gap-1">
                {files.map((f) => (
                    <li
                        key={f.path}
                        className={cn(
                            'flex items-center gap-2 rounded-sm px-2.5 py-1.5 text-xs',
                            f.problem ? 'bg-danger-soft/50' : 'bg-inset/60',
                        )}
                    >
                        <FileText
                            size={13}
                            className={cn(
                                'shrink-0',
                                f.problem ? 'text-danger' : 'text-text-tertiary',
                            )}
                        />
                        <span className="min-w-0 flex-1 truncate text-text" title={f.path}>
                            {f.name}
                        </span>
                        <span
                            className={cn(
                                'shrink-0',
                                f.problem ? 'text-danger' : 'text-text-tertiary',
                            )}
                        >
                            {f.problem ?? formatSize(f.size)}
                        </span>
                        <button
                            type="button"
                            aria-label={`不导 ${f.name}`}
                            onClick={() => onRemove(f.path)}
                            className="rounded-full p-0.5 text-text-tertiary hover:bg-border-subtle hover:text-text"
                        >
                            <X size={12} />
                        </button>
                    </li>
                ))}
            </ul>
        )}
    </div>
);

const TaskRow: React.FC<{
    instanceId: string;
    task: MaiBotMemoryTask;
    label: string | undefined;
    open: boolean;
    pollMs: number | undefined;
    onToggle: () => void;
}> = ({ instanceId, task: t, label, open, pollMs, onToggle }) => {
    const act = useMaiBotMemoryTaskAction(instanceId);
    const detail = useMaiBotMemoryTask(instanceId, open ? t.id : null, pollMs);
    const active = taskActive(t);
    const Icon = t.source === 'paste' ? ClipboardPaste : FileText;
    const title =
        label ??
        detail.data?.files.map((f) => f.name).join('、') ??
        (t.source === 'paste' ? '粘贴的一段' : `${t.file_count} 个文件`);
    const canRetry = t.status === 'failed' || t.status === 'done_with_errors';
    const meta = [
        t.total_chunks > 0 && `${t.done_chunks} / ${t.total_chunks} 块`,
        t.failed_chunks > 0 && `${t.failed_chunks} 块没成`,
        relativeTime(t.created_at),
        t.error,
    ].filter(Boolean);
    return (
        <div className="rounded-md border border-border-subtle bg-surface">
            <div className="flex items-center gap-3 px-3 py-2.5">
                <span className="inline-flex h-8 w-8 shrink-0 items-center justify-center rounded-sm bg-inset text-text-secondary">
                    <Icon size={15} />
                </span>
                {/* 标题独占一行：右栏窄，和状态挤在一行会被截得只剩一个字 */}
                <button
                    type="button"
                    onClick={onToggle}
                    className="min-w-0 flex-1 text-left focus-visible:outline-none"
                >
                    <span className="block truncate text-[13px] text-text" title={title}>
                        {title}
                    </span>
                    <span className="mt-1 flex min-w-0 items-center gap-1.5">
                        <Badge tone={STATUS_TONE[t.status]} className="shrink-0">
                            {STATUS_TEXT[t.status]}
                            {active && t.progress > 0 && ` ${Math.round(t.progress * 100)}%`}
                        </Badge>
                        <span className="truncate text-2xs text-text-tertiary">
                            {meta.join(' · ')}
                        </span>
                    </span>
                    {active && (
                        <Progress
                            className="mt-2"
                            size="sm"
                            value={t.progress * 100}
                            indeterminate={t.progress === 0}
                        />
                    )}
                </button>
                {active ? (
                    <Button
                        size="sm"
                        variant="ghost"
                        disabled={act.isPending || t.status === 'cancelling'}
                        onClick={() => act.mutate({ op: 'cancel', id: t.id })}
                    >
                        <X size={13} />
                        取消
                    </Button>
                ) : (
                    canRetry && (
                        <Button
                            size="sm"
                            variant="ghost"
                            title="只重跑没成的那几块"
                            disabled={act.isPending}
                            onClick={() => act.mutate({ op: 'retry', id: t.id })}
                        >
                            <RotateCcw size={13} />
                            重试
                        </Button>
                    )
                )}
            </div>
            {open && (
                <div className="border-t border-border-subtle px-3 py-2">
                    {detail.data ? (
                        <ul className="flex flex-col gap-1.5">
                            {detail.data.files.map((f, i) => (
                                <li key={i} className="flex flex-col gap-0.5 text-xs">
                                    <span className="flex items-center gap-2">
                                        <FileText
                                            size={12}
                                            className="shrink-0 text-text-tertiary"
                                        />
                                        <span className="min-w-0 flex-1 truncate text-text">
                                            {f.name}
                                        </span>
                                        <span className="shrink-0 text-2xs text-text-tertiary">
                                            {STATUS_TEXT[f.status]}
                                            {f.total_chunks > 0 &&
                                                ` · ${f.done_chunks} / ${f.total_chunks} 块`}
                                        </span>
                                    </span>
                                    {f.error && (
                                        <span className="pl-5 text-2xs text-danger">{f.error}</span>
                                    )}
                                    {f.warnings.map((w, j) => (
                                        <span key={j} className="pl-5 text-2xs text-warning">
                                            {w}
                                        </span>
                                    ))}
                                </li>
                            ))}
                        </ul>
                    ) : (
                        <p className="text-2xs text-text-tertiary">
                            {detail.isError ? '这个任务的详情拿不到了' : '正在读取…'}
                        </p>
                    )}
                </div>
            )}
        </div>
    );
};
