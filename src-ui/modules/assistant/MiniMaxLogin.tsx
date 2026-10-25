// 私有诊断配置的 MiniMax 认证入口。

import { Copy, KeyRound, Loader2 } from 'lucide-react';
import type { AssistantSessionInfo } from '../../core/domain/assistant/types';
import { isAssistantBusy } from '../../core/domain/assistant/session';
import { useAssistantLogin } from '../../hooks/assistant/useAssistantLogin';
import { Button } from '../../shared/ui';

export function MiniMaxLogin({ info }: { info: AssistantSessionInfo }) {
    const login = useAssistantLogin(info.id);
    if (info.provider !== 'miniMax' || isAssistantBusy(info.state)) return null;
    return (
        <section
            aria-label="MiniMax 登录"
            className="shrink-0 space-y-2 border-b border-border-subtle bg-inset/40 px-4 py-3 sm:px-6"
        >
            <div className="flex flex-wrap items-center gap-3">
                <Button
                    size="sm"
                    disabled={login.busy || login.preview}
                    onClick={() => {
                        void login.start();
                    }}
                >
                    {login.busy ? (
                        <Loader2 size={13} className="animate-spin" />
                    ) : login.terminalEnabled ? (
                        <KeyRound size={13} />
                    ) : (
                        <Copy size={13} />
                    )}
                    {login.terminalEnabled ? '登录 MiniMax' : '复制 MiniMax 登录命令'}
                </Button>
                <p className="text-xs leading-5 text-text-secondary">
                    {login.preview
                        ? '浏览器预览不执行登录。'
                        : login.terminalEnabled
                          ? '在本机终端完成此诊断会话的登录。'
                          : '内嵌终端已关闭，可复制命令后在本机终端运行。'}
                </p>
            </div>
            {login.error && (
                <p role="alert" className="break-words text-xs text-danger">
                    {login.error}
                </p>
            )}
            {login.feedback && (
                <p role="status" className="text-xs leading-5 text-text-secondary">
                    {login.feedback}
                </p>
            )}
            {login.command && (!login.terminalEnabled || login.error) && (
                <details className="text-xs text-text-secondary">
                    <summary className="cursor-pointer">手动运行命令（{login.shellLabel}）</summary>
                    <pre className="mt-2 max-h-32 overflow-auto whitespace-pre-wrap break-all rounded-sm bg-field p-3 font-mono select-text">
                        {login.command}
                    </pre>
                </details>
            )}
        </section>
    );
}
