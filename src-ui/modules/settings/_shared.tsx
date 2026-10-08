// 设置页各 Tab 共用件的统一入口：外部一律 `from '../_shared'`，实现按内聚度拆在 shared/ 下。
// shared/sections：分组骨架 + 标准行；shared/themePicker：主题选择器；
// shared/appearanceControls：动画/圆角三选段与速度滑块；shared/intervalSliders：各采样时长滑块。

export { FieldRow, SettingsSection, SettingsTabSections } from './shared/sections';
export { ThemePicker } from './shared/themePicker';
export {
    MotionLevelSegment,
    MotionSpeedSlider,
    RadiusStyleSegment,
} from './shared/appearanceControls';
export {
    BotRuntimeMetricsIntervalSlider,
    InfoBarDismissDurationSlider,
    InfoBarDismissSliderPresence,
    PerformanceMonitorIntervalSlider,
    RemoteHostHealthProbeIntervalSlider,
    TaskQueueCleanupDurationSlider,
} from './shared/intervalSliders';
