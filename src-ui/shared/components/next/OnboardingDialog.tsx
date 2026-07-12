// 新手引导：门禁式选择 + 完整分步说明。
// 只指路、不装组件；文案说人话；视觉走暖粉 hero + 进度点。

import { useEffect, useState, type ComponentType } from 'react';
import type { LucideProps } from 'lucide-react';
import {
    ArrowRight,
    BookOpen,
    Bot,
    Compass,
    Map,
    Package,
    Rocket,
    SkipForward,
    TriangleAlert,
} from 'lucide-react';
import {
    Button,
    Dialog,
    DialogContent,
    DialogDescription,
    DialogTitle,
} from '../../ui';
import { DialogStepTransition, MotionIcon } from '../../ui/motion';
import { cn } from '../../utils/cn';
import type { OnboardingDialogMode } from '../../../hooks/desktop/useOnboardingGate';
import {
    GoPreview,
    MapPreview,
    PathStoryPreview,
    TipsPreview,
    WelcomePreview,
} from './OnboardingPreviews';

export const ONBOARDING_GUIDE_STEP_IDS = [
    'welcome',
    'map',
    'path',
    'tips',
    'go',
] as const;

export type OnboardingGuideStepId = (typeof ONBOARDING_GUIDE_STEP_IDS)[number];

export interface OnboardingDialogProps {
    open: boolean;
    mode: OnboardingDialogMode;
    submitting: boolean;
    onExplore: () => void;
    onSkip: () => void;
    onCloseGuide: () => void;
    onGoComponents: () => void;
    onGoBots: () => void;
    /** 认路结束并继续整条引导（进组件页遮罩） */
    onFinish: () => void;
    /** 仅结束 Dialog、进主界面，不接遮罩 */
    onDismissToApp?: () => void;
}

type GuideVisual = 'welcome' | 'map' | 'path' | 'tips' | 'go';

type GuideFact = {
    label: string;
    text: string;
};

type GuideStep = {
    id: OnboardingGuideStepId;
    title: string;
    lead: string;
    /** 左栏要点，填满空白；写具体事，不写空话 */
    facts: readonly GuideFact[];
    icon: ComponentType<LucideProps>;
    visual: GuideVisual;
    showActions?: boolean;
};

const GUIDE_STEPS: readonly GuideStep[] = [
    {
        id: 'welcome',
        title: '三样东西',
        lead: 'Desktop 是控制台：装依赖、建 Bot、启停、看日志。协议端和 QQ 是另外两层。',
        icon: Compass,
        visual: 'welcome',
        facts: [
            {
                label: 'Desktop',
                text: '你现在打开的这个窗口。管组件、Bot、远端、通知。',
            },
            {
                label: 'NapCat / SnowLuma',
                text: '连 QQ 的协议端，装在「组件」里，由 Bot 拉起。',
            },
            {
                label: 'QQ 客户端',
                text: '原来的 QQ。扫码、登录都在它那边。',
            },
            {
                label: '右边预览',
                text: '主界面一角：左导航，右机器人列表卡。',
            },
        ],
    },
    {
        id: 'map',
        title: '侧栏里常用的两个',
        lead: '日常主要在「组件」和「机器人」之间切换。',
        icon: Map,
        visual: 'map',
        facts: [
            {
                label: '组件',
                text: '装 / 更新 Node、QQ、NapCat（或 SnowLuma）。进度在任务里。',
            },
            {
                label: '机器人',
                text: '建实例、启停、扫码、日志、WebUI。',
            },
            {
                label: '概览',
                text: '状态和通知的总览。',
            },
            {
                label: '远端 / 任务 / 设置',
                text: 'SSH 主机、安装进度、外观和掉线通知。',
            },
        ],
    },
    {
        id: 'path',
        title: '本机第一次大致这样',
        lead: '常见路径是：组件就绪 → 建 Bot → 启动扫码。远端和 Docker 是另一条线。',
        icon: Package,
        visual: 'path',
        facts: [
            {
                label: '① 组件',
                text: 'Node、QQ、NapCat 装在这里，进度在「任务」里。',
            },
            {
                label: '② 建 Bot',
                text: '本机 + NapCat（或你选的框架）。',
            },
            {
                label: '③ 启动扫码',
                text: '点启动，手机 QQ 扫码，状态变绿即登录成功。',
            },
        ],
    },
    {
        id: 'tips',
        title: '几个常见情况',
        lead: '多数卡点都和依赖、扫码、远端、通知有关。',
        icon: TriangleAlert,
        visual: 'tips',
        facts: [
            {
                label: '组件未齐就启动',
                text: 'Bot 能保存，启动时会提示缺依赖。',
            },
            {
                label: '扫码超时',
                text: '关掉再开一次登录即可。',
            },
            {
                label: '远端',
                text: '远程页加主机，组件页对那台机器装。本机与远端各自独立。',
            },
            {
                label: '掉线通知',
                text: '在设置里配通道；默认不会往群里发消息。',
            },
        ],
    },
    {
        id: 'go',
        title: '接下来是组件页',
        lead: '继续会进组件页，用遮罩介绍 NapCat、SnowLuma 和远端依赖。装完依赖后会再弹一层，说明创建 Bot。',
        icon: Rocket,
        visual: 'go',
        showActions: true,
        facts: [
            {
                label: '继续引导',
                text: '组件页：本机 NC / SL 对比，再看演示远端依赖。',
            },
            {
                label: '之后建 Bot',
                text: '组件遮罩结束后会再讲：新建入口、身份/连接、保存后启动扫码。',
            },
            {
                label: '以后想再看',
                text: '设置 → 关于 → 重新查看入门。',
            },
        ],
    },
] as const;

function GuideVisualBlock({ kind }: { kind: GuideVisual }) {
    switch (kind) {
        case 'welcome':
            return <WelcomePreview />;
        case 'map':
            return <MapPreview />;
        case 'path':
            return <PathStoryPreview />;
        case 'tips':
            return <TipsPreview />;
        case 'go':
            return <GoPreview />;
        default: {
            const _x: never = kind;
            return _x;
        }
    }
}

export function OnboardingDialog({
    open,
    mode,
    submitting,
    onExplore,
    onSkip,
    onCloseGuide: _onCloseGuide,
    onGoComponents,
    onGoBots,
    onFinish,
    onDismissToApp,
}: OnboardingDialogProps) {
    const [step, setStep] = useState(0);

    useEffect(() => {
        if (open && mode === 'guide') setStep(0);
    }, [open, mode]);

    const isChoice = mode === 'choice';
    const current = GUIDE_STEPS[step] ?? GUIDE_STEPS[0];
    const isLast = step >= GUIDE_STEPS.length - 1;

    return (
        <Dialog
            open={open}
            onOpenChange={() => {
                /* 门禁：忽略 Esc / 遮罩关闭 */
            }}
        >
            <DialogContent
                size="onboarding"
                hideClose
                dismissOnOutsideClick={false}
                onEscapeKeyDown={(e) => e.preventDefault()}
            >
                {isChoice ? (
                    <ChoiceBody
                        submitting={submitting}
                        onExplore={onExplore}
                        onSkip={onSkip}
                    />
                ) : (
                    <GuideBody
                        stepIndex={step}
                        step={current}
                        isLast={isLast}
                        submitting={submitting}
                        onBack={() => setStep((s) => Math.max(0, s - 1))}
                        onNext={() => {
                            // 最后一步主路径：继续整条引导（遮罩）
                            if (isLast) onFinish();
                            else setStep((s) => s + 1);
                        }}
                        onDismissToApp={onDismissToApp}
                        onGoComponents={onGoComponents}
                        onGoBots={onGoBots}
                        total={GUIDE_STEPS.length}
                    />
                )}
            </DialogContent>
        </Dialog>
    );
}

function ChoiceBody({
    submitting,
    onExplore,
    onSkip,
}: {
    submitting: boolean;
    onExplore: () => void;
    onSkip: () => void;
}) {
    return (
        <div className="flex min-h-0 flex-1 flex-col">
            <div
                className={cn(
                    'relative overflow-hidden border-b border-border-subtle/70',
                    'bg-[var(--surface-hero)] px-6 pb-5 pt-6 sm:px-8 sm:pt-7',
                )}
            >
                <div
                    aria-hidden
                    className="pointer-events-none absolute -right-8 -top-10 h-40 w-40 rounded-full bg-brand/15 blur-2xl"
                />
                <div
                    aria-hidden
                    className="pointer-events-none absolute bottom-0 left-1/3 h-24 w-48 rounded-full bg-brand/10 blur-2xl"
                />

                <div className="relative min-w-0 max-w-xl">
                    <DialogTitle className="font-display text-[1.5rem] font-bold leading-snug text-[var(--text-hero-title)] sm:text-[1.65rem]">
                        新手指引
                    </DialogTitle>
                    <DialogDescription className="mt-2.5 text-[13px] leading-relaxed text-text-secondary">
                        简单介绍 Desktop 是什么、侧栏有什么、本机第一个 Bot
                        大致怎么走。也可以直接进主界面，设置里还能再打开。
                    </DialogDescription>
                </div>
            </div>

            <div className="grid flex-1 grid-cols-1 gap-3 p-5 sm:grid-cols-2 sm:gap-3 sm:p-5">
                <ChoiceCard
                    title="看一遍介绍"
                    description="三层结构、侧栏、本机路径，以及组件页框架对比。"
                    icon={BookOpen}
                    disabled={submitting}
                    primary
                    onClick={onExplore}
                />
                <ChoiceCard
                    title="直接进主界面"
                    description="跳过介绍。设置 → 关于 可重新打开。"
                    icon={SkipForward}
                    disabled={submitting}
                    onClick={onSkip}
                />
            </div>
        </div>
    );
}

function ChoiceCard({
    title,
    description,
    icon: Icon,
    disabled,
    primary,
    onClick,
}: {
    title: string;
    description: string;
    icon: ComponentType<LucideProps>;
    disabled?: boolean;
    primary?: boolean;
    onClick: () => void;
}) {
    return (
        <button
            type="button"
            disabled={disabled}
            onClick={onClick}
            className={cn(
                'group flex h-full flex-col items-start gap-2.5 rounded-lg border p-4 text-left',
                'transition-colors duration-150',
                'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand focus-visible:ring-offset-2 focus-visible:ring-offset-elevated',
                'disabled:pointer-events-none disabled:opacity-50',
                primary
                    ? 'border-brand/40 bg-brand/[0.06] hover:border-brand hover:bg-brand/10'
                    : 'border-border-subtle bg-surface hover:border-border hover:bg-inset',
            )}
        >
            <span
                className={cn(
                    'inline-flex h-9 w-9 items-center justify-center rounded-md',
                    primary ? 'bg-brand/15 text-brand' : 'bg-inset text-text-secondary',
                )}
            >
                <MotionIcon
                    icon={Icon}
                    size={18}
                    strokeWidth={1.8}
                    playEnter={false}
                    className="text-inherit"
                />
            </span>
            <div className="min-w-0">
                <p className="text-[14px] font-semibold text-text">{title}</p>
                <p className="mt-1 text-[12.5px] leading-relaxed text-text-secondary">
                    {description}
                </p>
            </div>
            <span
                className={cn(
                    'mt-auto inline-flex items-center gap-1 text-[12px]',
                    primary ? 'text-brand' : 'text-text-tertiary',
                )}
            >
                {primary ? '继续' : '进入主界面'}
                <ArrowRight size={13} strokeWidth={2} />
            </span>
        </button>
    );
}

function GuideBody({
    stepIndex,
    step,
    isLast,
    submitting,
    onBack,
    onNext,
    onDismissToApp,
    onGoComponents,
    onGoBots,
    total,
}: {
    stepIndex: number;
    step: GuideStep;
    isLast: boolean;
    submitting: boolean;
    onBack: () => void;
    onNext: () => void;
    onDismissToApp?: () => void;
    onGoComponents: () => void;
    onGoBots: () => void;
    total: number;
}) {
    return (
        <div className="flex min-h-0 flex-1 flex-col">
            <div className="flex items-center gap-3 border-b border-border-subtle/60 px-5 py-3 sm:px-6">
                <div className="flex min-w-0 flex-1 items-center gap-1.5">
                    {GUIDE_STEPS.map((s, i) => {
                        const done = i < stepIndex;
                        const active = i === stepIndex;
                        return (
                            <div
                                key={s.id}
                                className={cn(
                                    'h-1 flex-1 rounded-full transition-colors duration-300',
                                    done || active ? 'bg-brand' : 'bg-inset',
                                )}
                                aria-hidden
                            />
                        );
                    })}
                </div>
                <span className="shrink-0 tabular-nums text-[11px] text-text-tertiary">
                    {stepIndex + 1}/{total}
                </span>
            </div>

            <div className="min-h-0 flex-1 overflow-y-auto px-5 py-4 sm:px-6 sm:py-5">
                <DialogStepTransition stepKey={step.id}>
                    {(() => {
                        // path / go / tips：标题+要点在上，预览通栏在下（避免左高右矮）
                        // welcome / map：左文右图
                        const stackVisual =
                            step.visual === 'path' ||
                            step.visual === 'go' ||
                            step.visual === 'tips';
                        const factCols =
                            step.visual === 'tips'
                                ? 'sm:grid-cols-2'
                                : step.visual === 'path' || step.visual === 'go'
                                    ? 'sm:grid-cols-3'
                                    : '';
                        return (
                            <div
                                className={cn(
                                    'gap-4 sm:gap-5',
                                    stackVisual
                                        ? 'grid grid-cols-1'
                                        : // flex + items-center 比 grid 更稳：预览块按内容高度，贴左栏中线
                                        'flex flex-col lg:flex-row lg:items-center lg:gap-6',
                                )}
                            >
                                <div
                                    className={cn(
                                        'flex min-w-0 flex-col',
                                        !stackVisual && 'lg:min-w-0 lg:flex-1',
                                    )}
                                >
                                    <h2 className="font-display text-[1.2rem] font-semibold leading-snug text-text sm:text-[1.28rem]">
                                        {step.title}
                                    </h2>
                                    <p className="mt-2 text-[13px] leading-relaxed text-text-secondary">
                                        {step.lead}
                                    </p>

                                    <div
                                        className={cn(
                                            'mt-3.5 rounded-md border border-border-subtle/80 bg-inset/35',
                                            'divide-y divide-border-subtle/70',
                                            stackVisual &&
                                            factCols &&
                                            `sm:grid ${factCols} sm:divide-x sm:divide-y-0`,
                                        )}
                                    >
                                        {step.facts.map((f) => (
                                            <div
                                                key={f.label}
                                                className="px-3 py-2.5 sm:px-3.5 sm:py-3"
                                            >
                                                <p className="text-[11px] font-semibold tracking-wide text-brand">
                                                    {f.label}
                                                </p>
                                                <p className="mt-1 text-[12.5px] leading-relaxed text-text-secondary">
                                                    {f.text}
                                                </p>
                                            </div>
                                        ))}
                                    </div>

                                    {step.showActions ? (
                                        <div className="mt-4 flex flex-col gap-2 sm:flex-row sm:items-stretch">
                                            <ActionTile
                                                icon={Package}
                                                title="去组件页"
                                                description="接着看 NC / SL 和远端依赖"
                                                primary
                                                disabled={submitting}
                                                onClick={onGoComponents}
                                                className="sm:flex-1"
                                            />
                                            <ActionTile
                                                icon={Bot}
                                                title="同样去组件页"
                                                description="同一条遮罩介绍，之后再去建 Bot"
                                                disabled={submitting}
                                                onClick={onGoBots}
                                                className="sm:max-w-[15.5rem] sm:flex-none"
                                            />
                                        </div>
                                    ) : null}
                                </div>

                                <div
                                    className={cn(
                                        'min-w-0',
                                        !stackVisual &&
                                        'w-full shrink-0 lg:w-[min(100%,22.5rem)] lg:flex-none xl:w-[24rem]',
                                    )}
                                >
                                    <GuideVisualBlock kind={step.visual} />
                                </div>
                            </div>
                        );
                    })()}
                </DialogStepTransition>
            </div>

            <div className="flex items-center justify-between gap-3 border-t border-border-subtle/60 px-5 py-3 sm:px-6">
                <Button
                    variant="ghost"
                    size="sm"
                    disabled={stepIndex === 0 || submitting}
                    onClick={onBack}
                >
                    上一步
                </Button>
                {isLast ? (
                    <div className="flex flex-wrap items-center justify-end gap-2">
                        <Button
                            variant="ghost"
                            size="sm"
                            disabled={submitting}
                            onClick={() => (onDismissToApp ?? onNext)()}
                        >
                            进入主界面
                        </Button>
                        <Button
                            variant="primary"
                            size="sm"
                            disabled={submitting}
                            onClick={onGoComponents}
                        >
                            去组件页
                            <ArrowRight size={14} strokeWidth={2} />
                        </Button>
                    </div>
                ) : (
                    <Button
                        variant="primary"
                        size="sm"
                        disabled={submitting}
                        onClick={onNext}
                    >
                        下一步
                        <ArrowRight size={14} strokeWidth={2} />
                    </Button>
                )}
            </div>
        </div>
    );
}

function ActionTile({
    icon: Icon,
    title,
    description,
    primary,
    disabled,
    onClick,
    className,
}: {
    icon: ComponentType<LucideProps>;
    title: string;
    description: string;
    primary?: boolean;
    disabled?: boolean;
    onClick: () => void;
    className?: string;
}) {
    return (
        <button
            type="button"
            disabled={disabled}
            onClick={onClick}
            className={cn(
                'flex items-start gap-3 rounded-md border px-3.5 py-3.5 text-left transition-colors',
                'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand focus-visible:ring-offset-2 focus-visible:ring-offset-elevated',
                'disabled:pointer-events-none disabled:opacity-50',
                primary
                    ? 'border-brand/40 bg-brand/10 hover:bg-brand/15'
                    : 'border-border-subtle bg-surface hover:bg-inset',
                className,
            )}
        >
            <span
                className={cn(
                    'mt-0.5 inline-flex h-8 w-8 shrink-0 items-center justify-center rounded-md',
                    primary ? 'bg-brand text-white' : 'bg-inset text-text-secondary',
                )}
            >
                <Icon size={16} strokeWidth={2} />
            </span>
            <span className="min-w-0">
                <span className="block text-[13px] font-semibold text-text">{title}</span>
                <span className="mt-0.5 block text-[12px] leading-relaxed text-text-secondary">
                    {description}
                </span>
            </span>
        </button>
    );
}
