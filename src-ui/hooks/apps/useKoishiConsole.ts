// 控制台功能（试聊沙盒 / 文件 / 数据库 / 指令）的查询与操作。
// 都是「实例在跑才有效」的控制台 WebSocket 调用；写操作成功后作废对应查询让页面刷新。

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { koishiService } from '../../core/services/koishi.service';
import { toAppConfigError } from '../../core/domain/apps/appConfigError';
import { pushInfoBar } from '../ui/globalInfoBarStore';
import { pushErrorBar } from '../ui/pushErrorBar';
import type {
    KoishiCommandRow,
    KoishiDatabaseTable,
    KoishiFileEntry,
    KoishiSandboxMessage,
} from '../../core/ipc/types';

const key = (id: string, ...rest: unknown[]) => ['koishiConsole', id, ...rest] as const;

export function useKoishiSandboxMessages(instanceId: string, running: boolean) {
    return useQuery<KoishiSandboxMessage[], Error>({
        queryKey: key(instanceId, 'sandbox'),
        queryFn: () => koishiService.sandboxMessages(instanceId),
        enabled: running,
        retry: false,
        refetchInterval: 1500,
    });
}

export function useKoishiSandboxSend(instanceId: string) {
    const qc = useQueryClient();
    return useMutation({
        mutationFn: (msg: { platform: string; user: string; channel: string; content: string }) =>
            koishiService.sandboxSend(instanceId, msg),
        onSuccess: () => {
            // Bot 多半要过一会才回：先刷一次，再补一次慢的
            void qc.invalidateQueries({ queryKey: key(instanceId, 'sandbox') });
            setTimeout(
                () => void qc.invalidateQueries({ queryKey: key(instanceId, 'sandbox') }),
                1200,
            );
        },
        onError: (e) => pushErrorBar({ title: '发送失败', content: toAppConfigError(e).message }),
    });
}

export function useKoishiExplorerTree(instanceId: string, running: boolean) {
    return useQuery<KoishiFileEntry[], Error>({
        queryKey: key(instanceId, 'files'),
        queryFn: () => koishiService.explorerTree(instanceId),
        enabled: running,
        retry: false,
    });
}

export function useKoishiFileOps(instanceId: string) {
    const qc = useQueryClient();
    const done = (what: string) => () => {
        pushInfoBar({ tone: 'success', title: what, autoDismissMs: 2000 });
        // 上游改完会刷新 explorer 推送，但推送要绕一圈，本地直接重取
        void qc.invalidateQueries({ queryKey: key(instanceId, 'files') });
    };
    const fail = (what: string) => (e: unknown) =>
        pushErrorBar({ title: `${what}失败`, content: toAppConfigError(e).message });
    return {
        write: useMutation({
            mutationFn: ({ path, content }: { path: string; content: string }) =>
                koishiService.explorerWrite(instanceId, path, content),
            onSuccess: done('已保存'),
            onError: fail('保存'),
        }),
        mkdir: useMutation({
            mutationFn: (path: string) => koishiService.explorerMkdir(instanceId, path),
            onSuccess: done('已新建'),
            onError: fail('新建'),
        }),
        remove: useMutation({
            mutationFn: (path: string) => koishiService.explorerRemove(instanceId, path),
            onSuccess: done('已删除'),
            onError: fail('删除'),
        }),
        rename: useMutation({
            mutationFn: ({ from, to }: { from: string; to: string }) =>
                koishiService.explorerRename(instanceId, from, to),
            onSuccess: done('已改名'),
            onError: fail('改名'),
        }),
    };
}

export function useKoishiDatabaseTables(instanceId: string, running: boolean) {
    return useQuery<KoishiDatabaseTable[], Error>({
        queryKey: key(instanceId, 'dbTables'),
        queryFn: () => koishiService.databaseTables(instanceId),
        enabled: running,
        retry: false,
    });
}

export function useKoishiDatabaseRows(
    instanceId: string,
    table: string | null,
    offset: number,
    limit: number,
) {
    return useQuery<Record<string, unknown>[], Error>({
        queryKey: key(instanceId, 'dbRows', table, offset, limit),
        queryFn: () => koishiService.databaseRows(instanceId, table!, offset, limit),
        enabled: !!table,
        retry: false,
    });
}

export function useKoishiCommands(instanceId: string, running: boolean) {
    return useQuery<KoishiCommandRow[], Error>({
        queryKey: key(instanceId, 'commands'),
        queryFn: () => koishiService.commands(instanceId),
        enabled: running,
        retry: false,
    });
}

export function useKoishiCommandOps(instanceId: string) {
    const qc = useQueryClient();
    const refresh = () => void qc.invalidateQueries({ queryKey: key(instanceId, 'commands') });
    return {
        update: useMutation({
            mutationFn: ({ name, config }: { name: string; config: Record<string, unknown> }) =>
                koishiService.commandUpdate(instanceId, name, config),
            onSuccess: () => {
                pushInfoBar({ tone: 'success', title: '指令配置已保存', autoDismissMs: 2000 });
                setTimeout(refresh, 600);
            },
            onError: (e) =>
                pushErrorBar({ title: '保存失败', content: toAppConfigError(e).message }),
        }),
        aliases: useMutation({
            mutationFn: ({ name, aliases }: { name: string; aliases: string[] }) =>
                koishiService.commandAliases(instanceId, name, aliases),
            onSuccess: () => setTimeout(refresh, 600),
            onError: (e) =>
                pushErrorBar({ title: '保存失败', content: toAppConfigError(e).message }),
        }),
    };
}
