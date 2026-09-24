import { useEffect, useRef } from 'react';
import type { NavBadges } from './frameworkUi';

/** 按内容比较再上报，免得每渲一次就 setState 把外壳拖进重渲；卸载时清空，别把圆点留给下一个实例。 */
export function useSyncNavBadges(onNavBadges: (badges: NavBadges) => void, badges: NavBadges) {
    const onRef = useRef(onNavBadges);
    onRef.current = onNavBadges;
    const key = JSON.stringify(badges);

    useEffect(() => {
        onRef.current(JSON.parse(key) as NavBadges);
    }, [key]);

    useEffect(() => () => onRef.current({}), []);
}
