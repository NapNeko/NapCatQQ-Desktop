import type { ReactNode } from 'react';
import { AlertTriangle, CheckCircle2, Info } from 'lucide-react';
import { cn } from '../../../../shared/utils/cn';

export function RuntimeNotice({
    tone,
    children,
}: {
    tone: 'ok' | 'warn' | 'neutral' | 'danger';
    children: ReactNode;
}) {
    const cls =
        tone === 'ok'
            ? 'border-success/25 bg-success-soft/60 text-success'
            : tone === 'danger'
              ? 'border-danger/25 bg-danger-soft/60 text-danger'
              : tone === 'warn'
                ? 'border-warning/25 bg-warning-soft/60 text-warning'
                : 'border-border-subtle bg-inset text-text-secondary';
    const Icon =
        tone === 'ok' ? CheckCircle2 : tone === 'danger' || tone === 'warn' ? AlertTriangle : Info;
    return (
        <div
            className={cn(
                'flex items-start gap-2 rounded-sm border px-3 py-2 text-2xs leading-relaxed',
                cls,
            )}
        >
            <Icon size={13} className="mt-0.5 shrink-0" />
            <div className="min-w-0 flex-1">{children}</div>
        </div>
    );
}
