import { BotMigrationDialog } from '../../dialogs/BotMigrationDialog';
import { QrCodeDialog } from './QrCodeDialog';
import { RemoteSnowLumaQrLoginDialog } from './RemoteSnowLumaQrLoginDialog';
import { pushInfoBar } from '../../../../hooks/ui/globalInfoBarStore';
import type { BotCardDialogsModel } from './useBotCardController';

export function BotCardDialogs(model: BotCardDialogsModel) {
    return (
        <>
            <BotMigrationDialog
                botId={model.botId}
                sourceRuntimeTarget={model.runtimeTarget}
                open={model.migrationOpen}
                onOpenChange={model.setMigrationOpen}
                onOpenLogin={model.onOpenLogin}
            />
            <QrCodeDialog
                open={model.qrOpen}
                onOpenChange={model.setQrOpen}
                qrcodeUrl={model.qrcodeUrl}
                botId={model.botId}
                isOnline={model.isOnline}
                invalidationReason={model.invalidationReason}
                onLoginSuccess={() => {
                    model.setQrOpen(false);
                    pushInfoBar({
                        tone: 'success',
                        title: '扫码登录成功',
                        content: `Bot ${model.botId} 已上线`,
                        autoDismissMs: 3000,
                    });
                }}
                onKicked={() => model.setQrOpen(false)}
            />
            {model.remoteQrEnabled && (
                <RemoteSnowLumaQrLoginDialog
                    initialRefresh
                    botId={model.botId}
                    open={model.remoteQrOpen && model.running}
                    onOpenChange={model.setRemoteQrOpen}
                    isOnline={model.remoteOnline}
                    onOpenVnc={model.onOpenVnc}
                />
            )}
        </>
    );
}
