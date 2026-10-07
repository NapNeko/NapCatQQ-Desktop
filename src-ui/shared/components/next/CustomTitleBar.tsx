// 主窗口标题栏，终端开关只随控制台加载。
import { useFeatureEnabled } from '../../../hooks/preferences/featureTogglesStore';
import { TerminalToggleButton } from './TerminalToggleButton';
import { TitleBarChrome } from './TitleBarChrome';

interface CustomTitleBarProps {
    className?: string;
    variant?: 'app' | 'window';
}

export function CustomTitleBar({ className, variant = 'app' }: CustomTitleBarProps) {
    const terminalEnabled = useFeatureEnabled('terminal');
    return (
        <TitleBarChrome className={className} tool={variant === 'window'}>
            {terminalEnabled && variant === 'app' && <TerminalToggleButton />}
        </TitleBarChrome>
    );
}

export default CustomTitleBar;
