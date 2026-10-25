// 详情页共享凭据状态，使故障提示可跳到正确的恢复入口。

import { createContext, useContext } from 'react';
import type { AppInstance } from '../../core/ipc/types';

export interface NeoBotPanelAccess {
    instance: AppInstance;
    /** 面板登录接口 1.2.4a1 起才有；更老的版本填了密码也代不了登录 */
    loginSupported: boolean;
    /** 本机密钥库里存过面板密码（存过不代表面板现在认它） */
    passwordRemembered: boolean;
    /** 切页（「去面板凭据」这类指路按钮用） */
    onGoTab: (tab: string) => void;
    /** 打开框架自带的 Web 控制台（外壳负责隧道与凭据） */
    onOpenWebUi: () => void;
}

export const NeoBotPanelContext = createContext<NeoBotPanelAccess | null>(null);

/** 外壳没挂 provider 时返回 null：测试里单独渲染一页也不必搭整套外壳。 */
export function useNeoBotPanelAccess(): NeoBotPanelAccess | null {
    return useContext(NeoBotPanelContext);
}
