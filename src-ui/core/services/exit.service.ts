import type { PrepareExitDesktopResponse } from '../ipc/generated/PrepareExitDesktopResponse';
import { invoke } from '../ipc/transport';

export type { PrepareExitDesktopResponse };

export async function prepareExitDesktop(): Promise<PrepareExitDesktopResponse> {
    return invoke<PrepareExitDesktopResponse>('prepare_exit_desktop');
}

export async function requestExitApp(): Promise<void> {
    return invoke<void>('request_exit_app');
}
