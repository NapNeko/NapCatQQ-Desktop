// 概览 CPU / RAM 卡头那句负载状态的分档。

export type LoadMetric = 'cpu' | 'ram';
export type LoadTone = 'calm' | 'normal' | 'warning' | 'danger';

export interface LoadLevel {
    label: string;
    tone: LoadTone;
}

interface LevelRow extends LoadLevel {
    below: number;
}

// 口吻跟 Hello 卡一样随意。内存常驻就占一大块，分档整体比 CPU 往上挪
const LEVELS: Record<LoadMetric, LevelRow[]> = {
    cpu: [
        { below: 25, label: '摸鱼中', tone: 'calm' },
        { below: 55, label: '干活中', tone: 'normal' },
        { below: 80, label: '有点忙', tone: 'warning' },
        { below: Infinity, label: '冒烟了', tone: 'danger' },
    ],
    ram: [
        { below: 50, label: '很宽敞', tone: 'calm' },
        { below: 75, label: '刚刚好', tone: 'normal' },
        { below: 90, label: '有点挤', tone: 'warning' },
        { below: Infinity, label: '快满了', tone: 'danger' },
    ],
};

export function loadLevel(metric: LoadMetric, percent: number): LoadLevel {
    const rows = LEVELS[metric];
    const hit = rows.find((row) => percent < row.below) ?? rows[rows.length - 1];
    return { label: hit.label, tone: hit.tone };
}
