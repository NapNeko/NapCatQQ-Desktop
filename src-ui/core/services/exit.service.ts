import { invoke } from '../ipc/transport';

export interface PrepareExitDesktopResponse {
    local_active: number;
    remote_active: number;
    can_exit: boolean;
}

export async function prepareExitDesktop(): Promise<PrepareExitDesktopResponse> {
    return invoke<PrepareExitDesktopResponse>('prepare_exit_desktop');
}

export async function requestExitApp(): Promise<void> {
    return invoke<void>('request_exit_app');
}