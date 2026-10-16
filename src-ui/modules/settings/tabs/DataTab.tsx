// 数据 Tab：数据根目录、整树迁移、配置导入导出。

import { useState } from 'react';
import { useConfigTransfer } from '../../../hooks/preferences/useConfigTransfer';
import { ConfigImportDialog } from '../ConfigImportDialog';
import { DataRootMigrateDialog } from '../DataRootMigrateDialog';
import { CacheCleanupDialog } from '../CacheCleanupDialog';
import { Button } from '../../../shared/ui';
import { FieldRow, SettingsSection, SettingsTabSections } from '../_shared';

interface Props {
    dataRoot: string;
    onOpenDataDir: () => Promise<string>;
    isOpeningDir: boolean;
}

export function DataTab({ dataRoot, onOpenDataDir, isOpeningDir }: Props) {
    const [migrateOpen, setMigrateOpen] = useState(false);
    const [cleanupOpen, setCleanupOpen] = useState(false);
    const {
        exportConfig,
        openImportWizard,
        importOpen,
        setImportOpen,
        onImported,
        isExporting: isExportingCfg,
        canRetryPreferences,
        retryPreferences,
        pendingFrameworks,
        frameworkPendingError,
        retryFrameworks,
        isRestoringFrameworks,
    } = useConfigTransfer();

    const handleOpen = async () => {
        try {
            await onOpenDataDir();
        } catch (err) {
            console.warn('打开数据目录失败:', err);
        }
    };

    return (
        <SettingsTabSections>
            <SettingsSection title="存储">
                <FieldRow
                    label="数据根目录"
                    description={
                        <span className="break-all font-mono text-[12px] text-text-tertiary">
                            {dataRoot}
                        </span>
                    }
                >
                    <div className="flex shrink-0 items-center gap-1.5">
                        <Button
                            variant="secondary"
                            size="sm"
                            onClick={handleOpen}
                            disabled={isOpeningDir}
                        >
                            打开
                        </Button>
                        <Button
                            variant="secondary"
                            size="sm"
                            onClick={() => setMigrateOpen(true)}
                            disabled={!dataRoot || dataRoot === '—'}
                        >
                            迁移
                        </Button>
                    </div>
                </FieldRow>
                <FieldRow
                    label="下载缓存"
                    description="查看并清理安装包、旧更新包和下载暂存文件。"
                    isLast
                >
                    <Button variant="secondary" size="sm" onClick={() => setCleanupOpen(true)}>
                        扫描与清理
                    </Button>
                </FieldRow>
            </SettingsSection>

            <SettingsSection
                title="配置备份"
                description="备份桌面端与框架配置、界面偏好。框架与插件程序、系统密钥库、SSH 私钥和聊天记录保持独立；换盘请使用数据根目录迁移。"
            >
                <FieldRow
                    label="导出当前配置"
                    description="包含应用设置、Bot、远端档案、实例与关联、框架核心与模型设置、插件配置、麦麦自定义提示词、聊天设置、API 调试工作区与收藏、SnowLuma 设置和界面偏好。"
                >
                    <Button
                        variant="secondary"
                        size="sm"
                        onClick={() => exportConfig()}
                        disabled={isExportingCfg}
                    >
                        {isExportingCfg ? '导出中…' : '导出 ZIP'}
                    </Button>
                </FieldRow>

                <FieldRow
                    label="导入配置"
                    isLast={
                        !canRetryPreferences && !pendingFrameworks.length && !frameworkPendingError
                    }
                >
                    <Button variant="secondary" size="sm" onClick={openImportWizard}>
                        打开导入向导
                    </Button>
                </FieldRow>
                {canRetryPreferences && (
                    <FieldRow
                        label="界面偏好待恢复"
                        description="配置文件已经导入，修复浏览器存储问题后可单独重试界面与终端偏好。"
                        isLast={!pendingFrameworks.length && !frameworkPendingError}
                    >
                        <Button variant="secondary" size="sm" onClick={retryPreferences}>
                            重试恢复界面偏好
                        </Button>
                    </FieldRow>
                )}
                {pendingFrameworks.length > 0 && (
                    <FieldRow
                        label="框架配置待恢复"
                        description={`已保留恢复副本：${pendingFrameworks.join('；')}。完成框架安装、停止实例并连接远端后，可单独重试。`}
                        isLast
                    >
                        <Button
                            variant="secondary"
                            size="sm"
                            onClick={retryFrameworks}
                            disabled={isRestoringFrameworks}
                        >
                            {isRestoringFrameworks ? '恢复中…' : '重试恢复框架配置'}
                        </Button>
                    </FieldRow>
                )}
                {frameworkPendingError && (
                    <p className="text-[12px] text-warning">
                        待恢复框架配置读取失败：{frameworkPendingError.message}
                    </p>
                )}
            </SettingsSection>

            <ConfigImportDialog
                open={importOpen}
                onOpenChange={setImportOpen}
                onImported={onImported}
            />

            <DataRootMigrateDialog
                open={migrateOpen}
                onOpenChange={setMigrateOpen}
                currentDataRoot={dataRoot}
            />
            <CacheCleanupDialog open={cleanupOpen} onOpenChange={setCleanupOpen} />
        </SettingsTabSections>
    );
}
