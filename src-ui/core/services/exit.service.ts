import type { PrepareExitDesktopResponse } from '../ipc/generated/PrepareExitDesktopResponse';
import { invoke } from '../ipc/transport';

export async function prepareExitDesktop(): Promise<PrepareExitDesktopResponse> {
    return invoke<PrepareExitDesktopResponse>('prepare_exit_desktop');
}

export async function requestExitApp(): Promise<void> {
    return invoke<void>('request_exit_app');
}

export async function consumePendingTrayExit(): Promise<boolean> {
    return invoke<boolean>('consume_pending_tray_exit');
}
