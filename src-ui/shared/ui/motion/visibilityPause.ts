// 页面不可见时暂停 GSAP 无限循环，回到前台再 resume。
// 不改动画参数，只省后台 WebView 的合成/计时开销。

type PausableAnim = {
    pause: () => void;
    resume: () => void;
};

export function bindVisibilityPause(
    anim: PausableAnim | null | undefined,
): () => void {
    if (!anim) return () => { };

    const onVis = () => {
        if (document.hidden) {
            anim.pause();
        } else {
            anim.resume();
        }
    };

    document.addEventListener('visibilitychange', onVis);
    if (document.hidden) {
        anim.pause();
    }

    return () => {
        document.removeEventListener('visibilitychange', onVis);
    };
}
