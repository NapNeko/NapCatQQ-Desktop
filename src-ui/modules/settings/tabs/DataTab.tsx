// 数据 Tab。数据根目录打开（已有 IPC）+ 迁移报告导出（已有 IPC）+ 配置导入导出。
//
// 配置导入导出走 useConfigTransfer：导出当前配置到用户选的目录、从用户选的目录导入。
// 文件对话框走 tauri-plugin-dialog（webview 不能用 <input type=file> 拿真实路径）。

import { useBootstrap } from '../../../hooks/bootstrap/useBootstrap';
import { useConfigTransfer } from '../../../hooks/preferences/useConfigTransfer';
import { Button } from '../../../shared/ui';
import { FieldRow } from '../_shared';

interface Props {
    dataRoot: string;
    onOpenDataDir: () => Promise<string>;
    isOpeningDir: boolean;
}

export function DataTab({ dataRoot, onOpenDataDir, isOpeningDir }: Props) {
    const { exportMigrationReport, isExporting } = useBootstrap();
    const { exportConfig, importConfig, isExporting: isExportingCfg, isImporting } =
        useConfigTransfer();

    const handleOpen = async () => {
        try {
            await onOpenDataDir();
        } catch (err) {
            // eslint-disable-next-line no-console
            console.warn('打开数据目录失败:', err);
        }
    };

    return (
        <>
            <FieldRow label="数据根目录" description={dataRoot}>
                <Button
                    variant="secondary"
                    size="sm"
                    onClick={handleOpen}
                    disabled={isOpeningDir}
                >
                    打开
                </Button>
            </FieldRow>

            <FieldRow
                label="导出当前配置"
                description="把应用配置 / Bot 配置 / 远端档案打包到所选目录（不含密码、SSH 私钥、token 等密钥）"
            >
                <Button
                    variant="secondary"
                    size="sm"
                    onClick={() => exportConfig()}
                    disabled={isExportingCfg}
                >
                    {isExportingCfg ? '导出中…' : '导出'}
                </Button>
            </FieldRow>

            <FieldRow
                label="导入配置"
                description="从所选目录读取配置包并合并到当前数据根；导入后需重启生效。密钥不随包导入，需重新配置"
            >
                <Button
                    variant="secondary"
                    size="sm"
                    onClick={() => importConfig()}
                    disabled={isImporting}
                >
                    {isImporting ? '导入中…' : '导入'}
                </Button>
            </FieldRow>

            <FieldRow
                label="导出迁移报告"
                description="导出上次从旧版 Python 迁移的诊断报告，排查迁移问题时用"
                isLast
            >
                <Button
                    variant="secondary"
                    size="sm"
                    onClick={() => exportMigrationReport()}
                    disabled={isExporting}
                >
                    {isExporting ? '导出中…' : '导出报告'}
                </Button>
            </FieldRow>
        </>
    );
}
