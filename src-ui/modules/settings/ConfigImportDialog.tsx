// 配置导入向导：选 ZIP/文件夹 → 扫描动画 → 预览 → 确认导入。

import { useGSAP } from '@gsap/react';
import gsap from 'gsap';
import { PackageCheck, Sparkles, Upload } from 'lucide-react';
import { useCallback, useRef, useState } from 'react';
import { configTransferService } from '../../core/services/config-transfer.service';
import type { ConfigImportPreview } from '../../core/ipc/types';
import { useMotion } from '../../hooks/preferences/useMotion';
import {
    Button,
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from '../../shared/ui';
import { cn } from '../../shared/utils/cn';
import { classifyDroppedPath, useTauriDropTarget } from './useTauriDropTarget';

type Phase = 'pick' | 'scan' | 'review' | 'import' | 'done' | 'error';

export interface ConfigImportDialogProps {
    open: boolean;
    onOpenChange: (open: boolean) => void;
    onImported: (result: { files: string[]; skipped: string[] }) => void;
}

export function ConfigImportDialog({
    open,
    onOpenChange,
    onImported,
}: ConfigImportDialogProps) {
    const m = useMotion();
    const [phase, setPhase] = useState<Phase>('pick');
    const [preview, setPreview] = useState<ConfigImportPreview | null>(null);
    const [errorMsg, setErrorMsg] = useState<string | null>(null);
    const scanRootRef = useRef<HTMLDivElement>(null);
    const reviewRef = useRef<HTMLDivElement>(null);
    const dropZoneRef = useRef<HTMLDivElement>(null);
    const [dropHint, setDropHint] = useState<string | null>(null);

    const reset = useCallback(() => {
        setPhase('pick');
        setPreview(null);
        setErrorMsg(null);
        setDropHint(null);
    }, []);

    const handleOpenChange = (next: boolean) => {
        if (!next) reset();
        onOpenChange(next);
    };

    const runPreview = async (sourcePath: string) => {
        setPhase('scan');
        setErrorMsg(null);
        try {
            await new Promise((r) => setTimeout(r, m.enabled ? 720 : 120));
            const p = await configTransferService.preview(sourcePath);
            setPreview(p);
            if (!p.can_import) {
                setErrorMsg(p.warnings[0] ?? '未识别到可导入的配置');
                setPhase('error');
                return;
            }
            setPhase('review');
        } catch (e) {
            setErrorMsg(e instanceof Error ? e.message : String(e));
            setPhase('error');
        }
    };

    const pickZip = async () => {
        const path = await configTransferService.pickZipSource();
        if (path) await runPreview(path);
    };

    const handleDropZoneClick = () => {
        void pickZip();
    };

    const onDropped = useCallback((path: string) => {
        const kind = classifyDroppedPath(path);
        setDropHint(kind === 'zip' ? '已识别：ZIP 配置包' : '已识别：文件夹');
        void runPreview(path);
    }, []);

    const { dragHover } = useTauriDropTarget(open && phase === 'pick', dropZoneRef, onDropped);

    const confirmImport = async () => {
        if (!preview?.can_import) return;
        setPhase('import');
        try {
            const result = await configTransferService.import(preview.source_path);
            setPhase('done');
            onImported(result);
        } catch (e) {
            setErrorMsg(e instanceof Error ? e.message : String(e));
            setPhase('error');
        }
    };

    useGSAP(
        () => {
            if (phase !== 'scan' || !scanRootRef.current || !m.enabled) return;
            const bars = scanRootRef.current.querySelectorAll('[data-scan-bar]');
            gsap.fromTo(
                bars,
                { scaleX: 0.12, opacity: 0.35 },
                {
                    scaleX: 1,
                    opacity: 1,
                    duration: 0.55,
                    stagger: 0.08,
                    ease: m.ease.enter,
                    transformOrigin: 'left center',
                },
            );
            gsap.to(bars, {
                opacity: 0.45,
                duration: 0.9,
                stagger: { each: 0.12, repeat: -1, yoyo: true },
                ease: 'sine.inOut',
            });
        },
        { dependencies: [phase, m.enabled], scope: scanRootRef },
    );

    useGSAP(
        () => {
            if (phase !== 'review' || !reviewRef.current || !m.enabled) return;
            const rows = reviewRef.current.querySelectorAll('[data-review-row]');
            gsap.fromTo(
                rows,
                { autoAlpha: 0, y: 10 },
                {
                    autoAlpha: 1,
                    y: 0,
                    duration: m.duration('base'),
                    stagger: 0.06,
                    ease: m.ease.enter,
                },
            );
        },
        { dependencies: [phase, m.enabled], scope: reviewRef },
    );

    return (
        <Dialog open={open} onOpenChange={handleOpenChange}>
            <DialogContent className="max-w-md gap-0 p-5 sm:max-w-lg">
                <DialogHeader className="mb-2">
                    <DialogTitle className="flex items-center gap-2 font-display">
                        <PackageCheck size={18} className="text-brand" />
                        导入配置
                    </DialogTitle>
                    <DialogDescription>
                        从 ZIP 包或文件夹恢复应用配置、Bot 与远端档案。密钥不会随包导入。
                    </DialogDescription>
                </DialogHeader>

                {phase === 'pick' && (
                    <div className="py-1">
                        <div ref={dropZoneRef}>
                            <button
                                type="button"
                                onClick={handleDropZoneClick}
                            className={cn(
                                'relative flex w-full min-h-[152px] cursor-pointer flex-col items-center justify-center gap-2.5 rounded-xl border-2 border-dashed px-5 py-6 text-center transition-colors duration-200',
                                'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand focus-visible:ring-offset-2 focus-visible:ring-offset-canvas',
                                dragHover
                                    ? 'border-brand bg-brand/8 ring-4 ring-brand/10'
                                    : 'border-border-subtle bg-inset/30 hover:border-brand/25 hover:bg-brand/[0.04]',
                            )}
                        >
                            <span
                                className={cn(
                                    'pointer-events-none flex h-14 w-14 items-center justify-center rounded-full transition-transform duration-200',
                                    dragHover
                                        ? 'scale-110 bg-brand/15 text-brand'
                                        : 'bg-surface text-text-secondary',
                                )}
                            >
                                <Upload size={26} strokeWidth={1.75} />
                            </span>
                            <div className="pointer-events-none space-y-1">
                                <p className="text-[14px] font-medium text-text">
                                    拖入 ZIP 或文件夹到此处
                                </p>
                                <p className="text-[12px] leading-relaxed text-text-tertiary">
                                    拖入 ZIP 或文件夹自动识别；点击选择 ZIP 包
                                </p>
                            </div>
                            {dropHint && (
                                <p className="pointer-events-none text-[11.5px] font-medium text-brand">
                                    {dropHint}
                                </p>
                            )}
                        </button>
                        </div>
                    </div>
                )}

                {phase === 'scan' && (
                    <div ref={scanRootRef} className="space-y-4 py-4">
                        <p className="text-[13px] font-medium text-text">正在分析导入源…</p>
                        <div className="space-y-3 rounded-lg border border-border-subtle bg-inset/40 p-4">
                            {[0.92, 0.68, 0.84, 0.55].map((w, i) => (
                                <div
                                    key={i}
                                    data-scan-bar
                                    className="h-2.5 rounded-pill bg-gradient-to-r from-brand/25 via-brand/50 to-brand/20"
                                    style={{ width: `${w * 100}%` }}
                                />
                            ))}
                        </div>
                        <p className="text-[12px] text-text-tertiary">
                            识别 config.json、bot.json、servers.json
                        </p>
                    </div>
                )}

                {phase === 'review' && preview && (
                    <div ref={reviewRef} className="space-y-3 py-2">
                        <div data-review-row className="rounded-lg border border-border-subtle bg-surface px-3 py-2.5">
                            <p className="text-[11px] font-medium uppercase tracking-wide text-text-tertiary">
                                来源
                            </p>
                            <p className="mt-1 break-all font-mono text-[11.5px] text-text-secondary">
                                {preview.source_path}
                            </p>
                            <p className="mt-1 text-[12px] text-text-tertiary">
                                {preview.source_kind === 'zip' ? 'ZIP 包' : '文件夹'}
                            </p>
                        </div>
                        <div data-review-row className="rounded-lg border border-border-subtle bg-surface px-3 py-2.5">
                            <p className="text-[11px] font-medium uppercase tracking-wide text-text-tertiary">
                                将导入
                            </p>
                            <ul className="mt-2 space-y-1">
                                {preview.files_found.map((f) => (
                                    <li
                                        key={f}
                                        className="flex items-center gap-2 text-[13px] text-text"
                                    >
                                        <Sparkles size={13} className="shrink-0 text-brand" />
                                        {f}
                                    </li>
                                ))}
                            </ul>
                        </div>
                        {preview.warnings.length > 0 && (
                            <p data-review-row className="text-[12px] text-warning">
                                {preview.warnings.join('；')}
                            </p>
                        )}
                        <p data-review-row className="text-[12px] leading-relaxed text-text-tertiary">
                            写回前会自动备份现有配置。导入后请重启应用；SSH 密码与 GitHub Token 需重新配置。
                        </p>
                    </div>
                )}

                {phase === 'import' && (
                    <div className="flex flex-col items-center gap-3 py-10">
                        <div className="h-10 w-10 animate-spin rounded-full border-2 border-brand/30 border-t-brand" />
                        <p className="text-[13px] text-text-secondary">正在写入配置…</p>
                    </div>
                )}

                {phase === 'done' && (
                    <div className="py-6 text-center">
                        <p className="text-[15px] font-medium text-text">导入完成</p>
                        <p className="mt-2 text-[13px] text-text-tertiary">重启应用后生效</p>
                    </div>
                )}

                {phase === 'error' && (
                    <div className="rounded-lg border border-danger/30 bg-danger/5 px-3 py-3">
                        <p className="text-[13px] text-danger">{errorMsg ?? '导入失败'}</p>
                    </div>
                )}

                {(phase === 'review' ||
                    phase === 'done' ||
                    phase === 'error') && (
                    <DialogFooter className="mt-3 gap-2 sm:gap-2">
                    {phase === 'review' && (
                        <>
                            <Button variant="ghost" onClick={() => setPhase('pick')}>
                                重选
                            </Button>
                            <Button variant="primary" onClick={() => void confirmImport()}>
                                确认导入
                            </Button>
                        </>
                    )}
                    {phase === 'done' && (
                        <Button variant="primary" onClick={() => handleOpenChange(false)}>
                            关闭
                        </Button>
                    )}
                    {phase === 'error' && (
                        <>
                            <Button variant="secondary" onClick={() => setPhase('pick')}>
                                重选
                            </Button>
                            <Button variant="ghost" onClick={() => handleOpenChange(false)}>
                                关闭
                            </Button>
                        </>
                    )}
                    </DialogFooter>
                )}
            </DialogContent>
        </Dialog>
    );
}