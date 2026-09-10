// 应用端浅路由：列表 ↔ 详情。退场动画结束前不丢 selectedId。

import React, { useEffect, useState } from 'react';
import { PageTransition } from '../../shared/ui/motion';
import { RouteErrorBoundary } from '../../shared/ui/RouteErrorBoundary';
import { AppInstanceListPage, type DetailTabHint } from './list/AppInstanceListPage';
import { AppInstancePageNext } from './detail/AppInstancePage.next';
import { WebUiAccountDialogHost } from './WebUiAccountDialog';
import type { AppRoute } from '../../shared/components/next/Sidebar';

type View = 'list' | 'detail';

const VIEW_ORDER: ReadonlyArray<View> = ['list', 'detail'];

export const AppsPageNext: React.FC<{ onNavigate?: (route: AppRoute) => void }> = ({ onNavigate }) => {
    const [view, setView] = useState<View>('list');
    const [selectedId, setSelectedId] = useState<string | null>(null);
    const [initialTab, setInitialTab] = useState<DetailTabHint | undefined>(undefined);

    const [displayedView, setDisplayedView] = useState<View>('list');
    const [subVisible, setSubVisible] = useState(true);
    const [subDirection, setSubDirection] = useState<-1 | 0 | 1>(0);

    useEffect(() => {
        if (view === displayedView) {
            if (!subVisible) setSubVisible(true);
            return;
        }
        const oldIdx = VIEW_ORDER.indexOf(displayedView);
        const newIdx = VIEW_ORDER.indexOf(view);
        setSubDirection(newIdx > oldIdx ? 1 : newIdx < oldIdx ? -1 : 0);
        setSubVisible(false);
    }, [view, displayedView, subVisible]);

    const handleSubExited = () => {
        setDisplayedView(view);
        if (view === 'list') {
            setSelectedId(null);
            setInitialTab(undefined);
        }
        setSubVisible(true);
    };

    return (
        <div className="flex h-full w-full flex-col">
            <PageTransition
                visible={subVisible}
                onExited={handleSubExited}
                direction={subDirection}
                className="flex min-h-0 flex-1 flex-col"
            >
                {displayedView === 'detail' && selectedId ? (
                    <RouteErrorBoundary title="应用实例详情加载失败">
                        <AppInstancePageNext
                            instanceId={selectedId}
                            initialTab={initialTab}
                            onBack={() => setView('list')}
                        />
                    </RouteErrorBoundary>
                ) : (
                    <RouteErrorBoundary title="应用端列表加载失败">
                        <AppInstanceListPage
                            onNavigate={onNavigate}
                            onOpenInstance={(id, tab) => {
                                setSelectedId(id);
                                setInitialTab(tab);
                                setView('detail');
                            }}
                        />
                    </RouteErrorBoundary>
                )}
            </PageTransition>
            <WebUiAccountDialogHost />
        </div>
    );
};

export default AppsPageNext;
