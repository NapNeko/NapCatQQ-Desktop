// 副作用标注：对接会改写 / 保存后要重启 / 端口会同步实例。只挂在这些字段的 label 上。

import { Badge, Tooltip, TooltipContent, TooltipTrigger } from '../../../shared/ui';

export const LinkDependentBadge: React.FC<{ linked?: boolean }> = ({ linked }) => (
    <Tooltip>
        <TooltipTrigger asChild>
            <Badge tone="brand" appearance="outline" className="cursor-help">
                对接
            </Badge>
        </TooltipTrigger>
        <TooltipContent>
            {linked ? '改完会自动更新协议 Bot 的反向 WS 连接' : '对接协议 Bot 时会写这一项'}
        </TooltipContent>
    </Tooltip>
);

export const RestartBadge: React.FC = () => (
    <Tooltip>
        <TooltipTrigger asChild>
            <Badge tone="warning" appearance="outline" className="cursor-help">
                需重启
            </Badge>
        </TooltipTrigger>
        <TooltipContent>Karin 不监听这个文件</TooltipContent>
    </Tooltip>
);

export const PortSyncBadge: React.FC = () => (
    <Tooltip>
        <TooltipTrigger asChild>
            <Badge tone="info" appearance="outline" className="cursor-help">
                同步端口
            </Badge>
        </TooltipTrigger>
        <TooltipContent>改完会同步实例端口；已对接时一并改协议 Bot 的连接地址。监听口需重启才换。</TooltipContent>
    </Tooltip>
);

export const FieldLabel: React.FC<{ text: string; children?: React.ReactNode }> = ({ text, children }) => (
    <span className="inline-flex items-center gap-1.5">
        {text}
        {children}
    </span>
);
