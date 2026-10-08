// 组件页应用端「新建 / 导入实例」对话框状态。从 ComponentsPage.next 外提；
// 组件页只按主机发起，hostId 已锁进请求里，对话框提交后的回执文案也集中在这。

import { useCallback, useState } from 'react';
import { globalInfoBarStore } from '../../hooks/ui/globalInfoBarStore';
import type { CreateInstanceRequest, ImportInstanceTarget } from '../apps';
import type {
    AppFrameworkManifest,
    AppInstance,
    CreateAppInstanceRequest,
    ImportAppInstanceRequest,
} from '../../core/ipc/types';

// 与 CreateInstanceDialog 的提交草稿结构一致（跨模块只走 apps 入口，draft 类型
// 不在入口上；对话框直接把 draft 传进来，结构化兼容即可）。
export interface CreateAppDraftPayload {
    frameworkId: string;
    hostId: string;
    displayName: string;
    port: number | null;
    installNow: boolean;
    installDirOverride: string | null;
    installRenderer: boolean;
    webuiUsername: string;
    webuiPassword: string;
    acceptTerms: boolean;
    version: string | null;
}

export interface ImportInstanceDraft {
    frameworkId: string;
    hostId: string;
    path: string;
    displayName: string;
}

// useAppInstances 返回值里对话框真正用到的那几个成员,按结构声明消费,
// 不在组件层复制 hook 的完整返回类型。
export interface AppInstanceDialogsApps {
    create: (req: CreateAppInstanceRequest) => Promise<AppInstance>;
    importInstance: (req: ImportAppInstanceRequest) => Promise<AppInstance>;
    install: (id: string, version?: string | null) => void;
}

export interface AppInstanceDialogsApi {
    createAppRequest: CreateInstanceRequest | null;
    importAppTarget: ImportInstanceTarget | null;
    handleCreateAppInstance: (manifest: AppFrameworkManifest, hostId: string) => void;
    handleImportAppInstance: (manifest: AppFrameworkManifest, hostId: string) => void;
    closeCreateDialog: () => void;
    closeImportDialog: () => void;
    submitImport: (draft: ImportInstanceDraft) => Promise<void>;
    submitCreate: (draft: CreateAppDraftPayload) => Promise<void>;
}

export function useAppInstanceDialogs(args: {
    hostNameOf: (hostId: string) => string;
    apps: AppInstanceDialogsApps;
}): AppInstanceDialogsApi {
    const { hostNameOf, apps } = args;
    const [createAppRequest, setCreateAppRequest] = useState<CreateInstanceRequest | null>(null);
    const [importAppTarget, setImportAppTarget] = useState<ImportInstanceTarget | null>(null);

    const handleCreateAppInstance = useCallback(
        (manifest: AppFrameworkManifest, hostId: string) => {
            setCreateAppRequest({ manifest, lockedHostId: hostId });
        },
        [],
    );

    const handleImportAppInstance = useCallback(
        (manifest: AppFrameworkManifest, hostId: string) => {
            setImportAppTarget({ manifest, lockedHostId: hostId });
        },
        [],
    );

    const submitImport = useCallback(
        async (draft: ImportInstanceDraft) => {
            const imported = await apps.importInstance({
                framework_id: draft.frameworkId,
                host_id: draft.hostId,
                path: draft.path,
                display_name: draft.displayName,
            });
            setImportAppTarget(null);
            globalInfoBarStore.push({
                key: `app-instance-imported:${imported.id}`,
                tone: 'success',
                title: `已接管 ${imported.display_name} · ${hostNameOf(draft.hostId)}`,
                content:
                    imported.state === 'running'
                        ? '已由桌面端启动'
                        : imported.state === 'not_installed'
                          ? '依赖未同步，先到「应用端」页安装'
                          : '到「应用端」页启动并对接协议 Bot。',
                autoDismissMs: 8_000,
            });
        },
        [apps, hostNameOf],
    );

    const submitCreate = useCallback(
        async (draft: CreateAppDraftPayload) => {
            const userPassword = createAppRequest?.manifest.webui_auth === 'user_password';
            const created = await apps.create({
                framework_id: draft.frameworkId,
                host_id: draft.hostId,
                display_name: draft.displayName,
                port: draft.port ?? undefined,
                install_dir: draft.installDirOverride || undefined,
                install_renderer: createAppRequest?.manifest.has_install_renderer
                    ? draft.installRenderer
                    : undefined,
                webui_username: userPassword ? draft.webuiUsername.trim() || undefined : undefined,
                webui_password: userPassword ? draft.webuiPassword || undefined : undefined,
                // 新建实例默认不随桌面端启动，要的话到详情页打开
                auto_start: false,
                accept_terms: createAppRequest?.manifest.terms.length
                    ? draft.acceptTerms
                    : undefined,
            });
            if (draft.installNow) apps.install(created.id, draft.version);
            setCreateAppRequest(null);
            globalInfoBarStore.push({
                key: `app-instance-created:${created.id}`,
                tone: 'success',
                title: `已创建 ${created.display_name} · ${hostNameOf(draft.hostId)}`,
                content: [
                    draft.installNow
                        ? '安装进度见任务队列；装好后到「应用端」页启动并对接协议 Bot。'
                        : '实例已登记但未安装；到「应用端」页可随时安装。',
                    userPassword && !draft.webuiPassword
                        ? 'WebUI 密码已随机生成，见实例详情「连接」页。'
                        : '',
                ]
                    .filter(Boolean)
                    .join(' '),
                autoDismissMs: 8_000,
            });
        },
        [apps, createAppRequest, hostNameOf],
    );

    return {
        createAppRequest,
        importAppTarget,
        handleCreateAppInstance,
        handleImportAppInstance,
        closeCreateDialog: () => setCreateAppRequest(null),
        closeImportDialog: () => setImportAppTarget(null),
        submitImport,
        submitCreate,
    };
}
