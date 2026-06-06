// motion 原子件 barrel。
// 业务页面只 import { GsapPresence, PageTransition, ListItem, ... } from 'shared/ui/motion'。

export { GsapPresence, type EnterFn, type ExitFn } from './GsapPresence';
export { DialogStepTransition } from './DialogStepTransition';
export { PageTransition } from './PageTransition';
export { ListItem } from './ListItem';
export { MotionCard } from './MotionCard';
export { StatusDot, type StatusDotTone } from './StatusDot';
export { Counter } from './Counter';
export { Shimmer } from './Shimmer';
export { MotionIcon, type MotionIconPreset } from './MotionIcon';
export { ActionMotionIcon } from './ActionMotionIcon';
export { SegmentMotionIcon } from './SegmentMotionIcon';
export {
    NAV_ROUTE_MOTION,
    segmentMotion,
    refreshMotion,
    EMPHASIS_MOTION,
    LIVE_MOTION,
    RESOURCE_MOTION,
    SETTINGS_MOTION,
    FAB_PRIMARY_MOTION,
    BATCH_MOTION,
    infoToneMotion,
} from './motionIconSemantics';
export { SplashConfetti } from './SplashConfetti';
