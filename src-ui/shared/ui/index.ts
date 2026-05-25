// shared 原子件统一导出口。features 层只从这里 import：
//   import { Button, Card, Badge } from '@/shared/ui';

export { Button, type ButtonProps } from './Button';
export {
    Card,
    CardHeader,
    CardTitle,
    CardDescription,
    CardFooter,
    type CardProps,
} from './Card';
export { Badge, BadgeDot, type BadgeProps } from './Badge';
export { Tabs, TabsList, TabsTrigger, TabsContent } from './Tabs';
export {
    TooltipProvider,
    Tooltip,
    TooltipTrigger,
    TooltipContent,
} from './Tooltip';
export {
    Dialog,
    DialogTrigger,
    DialogClose,
    DialogPortal,
    DialogOverlay,
    DialogContent,
    DialogHeader,
    DialogTitle,
    DialogDescription,
    DialogFooter,
} from './Dialog';
export { Spinner, type SpinnerProps } from './Spinner';
export { Progress, type ProgressProps } from './Progress';
export { InfoBar, type InfoBarProps } from './InfoBar';
export { InfoBarStack, type InfoBarStackItem } from './InfoBarStack';
