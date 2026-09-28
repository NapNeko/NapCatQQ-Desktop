// 启动前的上游条款同意框。原文取自实例目录（就是这次要启动的那一版），
// 不同意就不启动：上游会卡在终端里等「同意」，Desktop 没有终端可给。

import { ExternalLink } from 'lucide-react';
import {
    Button,
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from '../../shared/ui';
import { SimpleMarkdown } from '../../shared/ui/SimpleMarkdown';
import { ActionMotionIcon } from '../../shared/ui/motion';
import { useTermsDialog } from '../../hooks/apps/termsDialogStore';
import { useOpenExternal } from '../../hooks/useOpenExternal';

export const TermsConsentDialogHost: React.FC = () => {
    const state = useTermsDialog();
    const open = useOpenExternal();
    return (
        <Dialog open={state !== null} onOpenChange={(o) => !o && state?.settle(false)}>
            <DialogContent size="sheet">
                {state && (
                    <>
                        <DialogHeader>
                            <DialogTitle>启动前请阅读并同意</DialogTitle>
                            <DialogDescription>
                                {state.instanceName} 的上游条款有新版本或还没同意过，同意后才会启动。
                            </DialogDescription>
                        </DialogHeader>
                        <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto pr-1">
                            {state.terms.map((t) => (
                                <section key={t.id} className="flex flex-col gap-2">
                                    <div className="flex items-center justify-between gap-2">
                                        <h3 className="text-sm font-semibold text-text">{t.title}</h3>
                                        <Button variant="ghost" size="sm" onClick={() => open(t.url)}>
                                            <ActionMotionIcon icon={ExternalLink} size={13} />
                                            在网页中查看
                                        </Button>
                                    </div>
                                    <div className="max-h-72 overflow-y-auto rounded-md border border-border-subtle bg-inset/40 px-4 py-3">
                                        <SimpleMarkdown text={t.text} onOpenLink={open} />
                                    </div>
                                </section>
                            ))}
                        </div>
                        <DialogFooter>
                            <Button variant="ghost" size="sm" onClick={() => state.settle(false)}>
                                不同意
                            </Button>
                            <Button variant="primary" size="sm" onClick={() => state.settle(true)}>
                                同意并启动
                            </Button>
                        </DialogFooter>
                    </>
                )}
            </DialogContent>
        </Dialog>
    );
};
