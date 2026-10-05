// 两种托盘共用外框、标题和操作行，主题与密度由同一处维护。
import { forwardRef, type ButtonHTMLAttributes, type HTMLAttributes, type ReactNode } from 'react';
import { cn } from '../../shared/utils/cn';

export const TrayPanelSurface = forwardRef<HTMLDivElement, HTMLAttributes<HTMLDivElement>>(
    ({ className, children, ...props }, ref) => <div className="flex min-h-full w-full flex-col overflow-hidden bg-elevated select-none">
        <div ref={ref} className={cn('flex w-full flex-col', className)} {...props}>{children}</div>
    </div>,
);
TrayPanelSurface.displayName = 'TrayPanelSurface';

export function TrayPanelHeader({ icon, title, children }: { icon: ReactNode; title: string; children: ReactNode }) {
    return <div className="flex items-center gap-2.5 px-3 pb-1 pt-2.5">
        {icon}
        <div className="flex min-w-0 flex-1 flex-col gap-0.5">
            <span className="truncate whitespace-nowrap text-[12.5px] font-semibold leading-none text-text">{title}</span>
            <span className="flex items-center gap-1.5 text-[11px] leading-none text-text-tertiary">{children}</span>
        </div>
    </div>;
}

export function TrayPanelSeparator() {
    return <div role="separator" className="mx-2 my-0.5 border-t border-border-subtle" />;
}

type TrayPanelActionProps = Omit<ButtonHTMLAttributes<HTMLButtonElement>, 'title'> & {
    icon: ReactNode;
    title: string;
    danger?: boolean;
    trailing?: ReactNode;
};

export function TrayPanelAction({ icon, title, danger, trailing, className, ...props }: TrayPanelActionProps) {
    return <button type="button" role="menuitem" className={cn(
        'group flex w-full items-center gap-2.5 rounded-md px-2.5 py-[6px] text-left cursor-pointer transition-colors duration-100',
        'focus-visible:outline-none disabled:cursor-default disabled:opacity-50',
        danger ? 'hover:bg-danger-soft focus-visible:bg-danger-soft' : 'hover:bg-brand-soft focus-visible:bg-brand-soft',
        className,
    )} {...props}>
        <span className={cn('flex shrink-0 items-center justify-center transition-colors',
            danger ? 'text-danger' : 'text-text-secondary group-hover:text-brand group-focus-visible:text-brand')}>{icon}</span>
        <span className={cn('min-w-0 flex-1 truncate text-[12.5px] leading-none',
            danger ? 'text-danger' : 'text-text group-hover:text-brand group-focus-visible:text-brand')}>{title}</span>
        {trailing}
    </button>;
}
