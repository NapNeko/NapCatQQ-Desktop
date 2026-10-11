// 仅显式 ?chatMotionProbe=1 采样，不在正常聊天中跑逐帧诊断。
export function startConversationMotionProbe(scope: HTMLElement, snapshotNodes: number) {
    if (!new URLSearchParams(location.search).has('chatMotionProbe'))
        return { committed: () => {}, finish: () => {}, captured: (_nodes: number) => {} };
    const started = performance.now();
    let committedAt = started;
    let mountMs = 0;
    let lastFrame = started;
    let maxGap = 0;
    let frames = 0;
    let nodes = snapshotNodes;
    let raf = 0;
    const slowFrames: PerformanceEntry[] = [];
    const sample = (now: number) => {
        maxGap = Math.max(maxGap, now - lastFrame);
        lastFrame = now;
        frames++;
        raf = requestAnimationFrame(sample);
    };
    raf = requestAnimationFrame(sample);
    const type = PerformanceObserver.supportedEntryTypes.includes('long-animation-frame')
        ? 'long-animation-frame'
        : 'longtask';
    const observer = new PerformanceObserver((list) => {
        for (const entry of list.getEntries())
            if (entry.startTime + entry.duration >= started) slowFrames.push(entry);
    });
    observer.observe({ type });
    let finished = false;
    return {
        captured(value: number) {
            nodes = value;
        },
        committed() {
            committedAt = performance.now();
            mountMs = committedAt - started;
        },
        finish() {
            if (finished) return;
            finished = true;
            cancelAnimationFrame(raf);
            const longEntries = [...slowFrames, ...observer.takeRecords()];
            observer.disconnect();
            const profile = {
                durationMs: Math.round(performance.now() - started),
                readyMs: Math.round(mountMs),
                animationMs: Math.round(performance.now() - committedAt),
                mainThreadFrames: frames,
                maxMainThreadGapMs: Math.round(maxGap),
                longFrameCount: longEntries.length,
                longestFrameMs: Math.round(
                    Math.max(0, ...longEntries.map((entry) => entry.duration)),
                ),
                longFrames: longEntries
                    .sort((a, b) => b.duration - a.duration)
                    .slice(0, 3)
                    .map(frameDetail),
                snapshotNodes: nodes,
            };
            const history: unknown[] = JSON.parse(scope.dataset.chatMotionProfile ?? '[]');
            scope.dataset.chatMotionProfile = JSON.stringify([...history.slice(-11), profile]);
            console.debug('CHAT_MOTION_PROFILE', JSON.stringify(profile));
        },
    };
}

function frameDetail(entry: PerformanceEntry) {
    const frame = entry as PerformanceEntry & {
        renderStart?: number;
        scripts?: Array<{
            duration: number;
            invoker?: string;
            sourceURL?: string;
            sourceFunctionName?: string;
            forcedStyleAndLayoutDuration?: number;
        }>;
    };
    return {
        durationMs: Math.round(entry.duration),
        renderMs: frame.renderStart
            ? Math.round(entry.startTime + entry.duration - frame.renderStart)
            : 0,
        scripts: frame.scripts
            ?.map((script) => ({
                durationMs: Math.round(script.duration),
                layoutMs: Math.round(script.forcedStyleAndLayoutDuration ?? 0),
                invoker: script.invoker,
                source: script.sourceURL?.split('/').pop(),
                function: script.sourceFunctionName,
            }))
            .sort((a, b) => b.durationMs - a.durationMs)
            .slice(0, 3),
    };
}

export function recordConversationTimelineMount(frame: HTMLElement | null) {
    if (!frame || !new URLSearchParams(location.search).has('chatMotionProbe')) return;
    frame.dataset.chatTimelineMounts = String(Number(frame.dataset.chatTimelineMounts ?? 0) + 1);
}
