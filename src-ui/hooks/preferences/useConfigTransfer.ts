// 配置导入导出 hook。
//
// 导出 / 导入各是一次性动作（弹目录对话框 → 后端处理），用 mutation 包一层，
// 结果走全局 InfoBar 反馈。用户取消对话框时 service 返回 null，这里静默收场。
// 导入成功后提示需重启生效（BotManager / ServerManager 启动期才加载配置）。

import { useMutation } from '@tanstack/react-query';
import { configTransferService } from '../../core/services/config-transfer.service';
import { pushInfoBar } from '../ui/globalInfoBarStore';

export function useConfigTransfer() {
    const exportMutation = useMutation({
        mutationFn: configTransferService.export,
        onSuccess: (result) => {
            if (!result) return; // 用户取消
            const files = result.files.length ? result.files.join('、') : '无可导出项';
            pushInfoBar({
                key: 'config-export',
                tone: 'success',
                title: '配置已导出',
                content: `${files} → ${result.export_dir}`,
            });
        },
        onError: (err: Error) => {
            pushInfoBar({
                key: 'config-export',
                tone: 'danger',
                title: '导出失败',
                content: err.message || String(err),
            });
        },
    });

    const importMutation = useMutation({
        mutationFn: configTransferService.import,
        onSuccess: (result) => {
            if (!result) return; // 用户取消
            if (!result.files.length) {
                pushInfoBar({
                    key: 'config-import',
                    tone: 'warning',
                    title: '未导入任何配置',
                    content: '来源目录里没有可识别的配置文件',
                });
                return;
            }
            const skippedNote = result.skipped.length
                ? `；未覆盖：${result.skipped.join('、')}`
                : '';
            pushInfoBar({
                key: 'config-import',
                tone: 'success',
                title: '配置已导入，重启后生效',
                content: `已导入：${result.files.join('、')}${skippedNote}。密钥不随包导入，需重新配置`,
            });
        },
        onError: (err: Error) => {
            pushInfoBar({
                key: 'config-import',
                tone: 'danger',
                title: '导入失败',
                content: err.message || String(err),
            });
        },
    });

    return {
        exportConfig: () => exportMutation.mutate(),
        importConfig: () => importMutation.mutate(),
        isExporting: exportMutation.isPending,
        isImporting: importMutation.isPending,
    };
}
