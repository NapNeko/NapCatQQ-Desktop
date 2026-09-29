// 远端 QQ 运行依赖：探测、安装、记住 sudo 密码。结果只给发起方用，不进缓存；
// 组件页按主机自己记探测结果，安装弹框自己管阶段。

import { useCallback } from 'react';
import { useMutation } from '@tanstack/react-query';
import { componentService } from '../../core/services/component.service';

export function useQqDependencyOps() {
    const detect = useMutation({ mutationFn: componentService.detectQqDependencies });
    const install = useMutation({
        mutationFn: (args: { hostId: string; packages: string[]; sudoPassword?: string }) =>
            componentService.installQqDependencies(args.hostId, args.packages, args.sudoPassword),
    });
    const remember = useMutation({
        mutationFn: (args: { serverId: string; password: string }) =>
            componentService.rememberSudoPassword(args.serverId, args.password),
    });

    const detectAsync = detect.mutateAsync;
    const installAsync = install.mutateAsync;
    const rememberAsync = remember.mutateAsync;

    return {
        detectQqDependencies: detectAsync,
        installQqDependencies: useCallback(
            (hostId: string, packages: string[], sudoPassword?: string) =>
                installAsync({ hostId, packages, sudoPassword }),
            [installAsync],
        ),
        rememberSudoPassword: useCallback(
            (serverId: string, password: string) => rememberAsync({ serverId, password }),
            [rememberAsync],
        ),
    };
}
