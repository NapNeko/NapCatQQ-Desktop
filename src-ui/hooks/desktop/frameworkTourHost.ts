// AppNext 注册框架 tour；去组件 / 设置关于 通过 request 触发。

export type FrameworkTourRequest = {
    /** full = 本机 + 演示远端 + 创建 Bot；也可只跑某一段 */
    mode?: 'full' | 'local' | 'remote' | 'bots';
};

type StartFn = (req?: FrameworkTourRequest) => void | Promise<void>;

let startFn: StartFn | null = null;

export function registerFrameworkTourHost(fn: StartFn | null): void {
    startFn = fn;
}

export async function requestFrameworkTour(req?: FrameworkTourRequest): Promise<void> {
    if (!startFn) {
        throw new Error('框架引导尚未就绪，请先打开主界面');
    }
    await startFn(req);
}
