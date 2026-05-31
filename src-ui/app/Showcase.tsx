// dev-only 路由：原子件 living showcase。
// 由 Sidebar 在 showShowcase=true 时显示入口。生产 build 仍可访问，但默认入口隐藏。

import React, { useState } from 'react';
import { Bot, Cog, FileText, Globe, Play, RotateCw, Square, Trash2 } from 'lucide-react';
import {
    Badge,
    Button,
    Card,
    CardDescription,
    CardFooter,
    CardHeader,
    CardTitle,
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
    DialogTrigger,
    Spinner,
    Tabs,
    TabsContent,
    TabsList,
    TabsTrigger,
    Tooltip,
    TooltipContent,
    TooltipTrigger,
} from '../shared/ui';
import catGirlSvg from '../assets/cat_girl.svg';

export const Showcase: React.FC = () => {
    const [activeTab, setActiveTab] = useState('basic');

    return (
        <div className="scrollbar-hide flex min-h-0 flex-1 flex-col gap-6 overflow-y-auto pb-8 pr-1">
            <header>
                <p className="text-2xs uppercase tracking-widest text-text-tertiary">showcase</p>
                <h1 className="font-display text-xl font-semibold text-text">原子件总览</h1>
                <p className="mt-1 text-sm text-text-secondary">
                    7 件套：Button / Card / Badge / Tabs / Tooltip / Dialog / Spinner。改样式时改这里看效果最快。
                </p>
            </header>

            {/* Hero + mascot */}
            <Card variant="hero" padding="xl" className="relative overflow-visible">
                <div className="max-w-md">
                    <CardTitle>Badge tone × appearance</CardTitle>
                    <CardDescription className="mt-1">
                        一张 BotCard 同时挂 4-6 枚徽章是常态，所以紧凑度 / 颜色辨识必须扛得住堆叠。
                    </CardDescription>
                    <div className="mt-5 flex flex-wrap gap-2">
                        <Badge tone="success" appearance="soft" dot>在线 · 10086421</Badge>
                        <Badge tone="brand" appearance="outline">NapCat</Badge>
                        <Badge tone="warning" appearance="soft">等待扫码</Badge>
                        <Badge tone="danger" appearance="solid">daemon crashed</Badge>
                        <Badge tone="info" appearance="soft">pending restart</Badge>
                        <Badge tone="neutral" appearance="outline">r12 / gen 3</Badge>
                    </div>
                </div>
                <img
                    src={catGirlSvg}
                    alt="napcat mascot"
                    className="pointer-events-none absolute -right-2 -top-7 h-[225px] w-[150px] select-none"
                />
            </Card>

            {/* Button matrix */}
            <Card>
                <CardHeader>
                    <div>
                        <CardTitle>Button</CardTitle>
                        <CardDescription>4 variant × 4 size。tooltip 包裹的 icon button 是 BotCard 的主形态。</CardDescription>
                    </div>
                </CardHeader>

                <div className="flex flex-wrap items-center gap-2">
                    <Button variant="primary">主操作</Button>
                    <Button variant="secondary">次要</Button>
                    <Button variant="ghost">幽灵</Button>
                    <Button variant="danger">危险</Button>
                    <Button variant="primary" disabled>禁用态</Button>
                    <Button variant="primary"><Spinner size="xs" />启动中…</Button>
                </div>

                <div className="mt-3 flex flex-wrap items-center gap-2">
                    <Button size="sm" variant="primary">small</Button>
                    <Button size="md" variant="primary">medium</Button>
                    <Button size="lg" variant="primary">large</Button>
                </div>

                <div className="mt-3 flex items-center gap-1">
                    <Tooltip>
                        <TooltipTrigger asChild>
                            <Button size="icon" variant="ghost" aria-label="启动 Bot">
                                <Play size={16} className="text-success" />
                            </Button>
                        </TooltipTrigger>
                        <TooltipContent>启动 Bot</TooltipContent>
                    </Tooltip>
                    <Tooltip>
                        <TooltipTrigger asChild>
                            <Button size="icon" variant="ghost" aria-label="停止 Bot">
                                <Square size={14} className="text-danger" />
                            </Button>
                        </TooltipTrigger>
                        <TooltipContent>停止 Bot</TooltipContent>
                    </Tooltip>
                    <Tooltip>
                        <TooltipTrigger asChild>
                            <Button size="icon" variant="ghost" aria-label="日志">
                                <FileText size={16} />
                            </Button>
                        </TooltipTrigger>
                        <TooltipContent>查看日志</TooltipContent>
                    </Tooltip>
                    <Tooltip>
                        <TooltipTrigger asChild>
                            <Button size="icon" variant="ghost" aria-label="WebUI">
                                <Globe size={16} />
                            </Button>
                        </TooltipTrigger>
                        <TooltipContent>打开 WebUI</TooltipContent>
                    </Tooltip>
                    <Tooltip>
                        <TooltipTrigger asChild>
                            <Button size="icon" variant="ghost" aria-label="配置">
                                <Cog size={16} />
                            </Button>
                        </TooltipTrigger>
                        <TooltipContent>编辑配置</TooltipContent>
                    </Tooltip>
                </div>
            </Card>

            {/* Tabs + Dialog */}
            <div className="grid grid-cols-12 gap-6">
                <Card className="col-span-7" padding="md">
                    <CardHeader>
                        <div>
                            <CardTitle>Tabs</CardTitle>
                            <CardDescription>底部 2px brand 强调线，BotConfigPage 三段式表单将由它承载。</CardDescription>
                        </div>
                    </CardHeader>
                    <Tabs value={activeTab} onValueChange={setActiveTab}>
                        <TabsList>
                            <TabsTrigger value="basic">基本配置</TabsTrigger>
                            <TabsTrigger value="connect">协议连接</TabsTrigger>
                            <TabsTrigger value="advanced">高阶优化</TabsTrigger>
                        </TabsList>
                        <TabsContent value="basic">
                            <Card variant="inset" padding="md">
                                <p className="text-sm text-text-secondary">
                                    basic tab 内容占位。
                                </p>
                            </Card>
                        </TabsContent>
                        <TabsContent value="connect">
                            <Card variant="inset" padding="md">
                                <p className="text-sm text-text-secondary">connect tab 占位。</p>
                            </Card>
                        </TabsContent>
                        <TabsContent value="advanced">
                            <Card variant="inset" padding="md">
                                <p className="text-sm text-text-secondary">advanced tab 占位。</p>
                            </Card>
                        </TabsContent>
                    </Tabs>
                </Card>

                <Card className="col-span-5" variant="outlined" padding="md">
                    <CardHeader>
                        <div>
                            <CardTitle>Dialog</CardTitle>
                            <CardDescription>批量删除 / 单 Bot 删除确认场景。</CardDescription>
                        </div>
                        <Bot size={18} className="text-text-tertiary" />
                    </CardHeader>

                    <Dialog>
                        <DialogTrigger asChild>
                            <Button variant="danger">
                                <Trash2 size={14} />
                                打开删除对话框
                            </Button>
                        </DialogTrigger>
                        <DialogContent>
                            <DialogHeader>
                                <DialogTitle>确认删除该实例？</DialogTitle>
                                <DialogDescription>
                                    你即将彻底删除 Bot 实例 <span className="font-mono text-text">10086421</span> 的全部配置。
                                    运行中的 Bot 会先被强制停止。此操作不可撤销。
                                </DialogDescription>
                            </DialogHeader>
                            <DialogFooter>
                                <Button variant="secondary">取消</Button>
                                <Button variant="danger">彻底删除</Button>
                            </DialogFooter>
                        </DialogContent>
                    </Dialog>

                    <CardFooter>
                        <span className="text-2xs text-text-tertiary">键盘 Esc 可关闭，Tab 焦点循环</span>
                    </CardFooter>
                </Card>
            </div>

            <Card variant="inset" padding="md">
                <CardHeader>
                    <div>
                        <CardTitle>Spinner</CardTitle>
                        <CardDescription>4 size × 3 tone。</CardDescription>
                    </div>
                </CardHeader>
                <div className="flex items-center gap-6">
                    <div className="flex items-center gap-2"><Spinner size="xs" /><span className="text-xs text-text-secondary">xs</span></div>
                    <div className="flex items-center gap-2"><Spinner size="sm" /><span className="text-xs text-text-secondary">sm</span></div>
                    <div className="flex items-center gap-2"><Spinner size="md" /><span className="text-xs text-text-secondary">md</span></div>
                    <div className="flex items-center gap-2"><Spinner size="lg" tone="brand" /><span className="text-xs text-text-secondary">lg / brand</span></div>
                    <div className="flex items-center gap-2 text-text-secondary">
                        <RotateCw size={14} /><span className="text-xs">单独看动画也对齐节拍</span>
                    </div>
                </div>
            </Card>
        </div>
    );
};

export default Showcase;
