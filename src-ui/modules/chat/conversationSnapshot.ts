// 快照只保留显示内容；播放器换成当前画面，不再创建第二条媒体读取。
export function createConversationSnapshot(body: HTMLElement, frame: HTMLElement): HTMLElement {
    const frozen = body.cloneNode(true) as HTMLElement;
    frozen.dataset.chatSnapshot = 'true';
    frozen.setAttribute('aria-hidden', 'true');
    frozen.inert = true;
    frozen.style.width = `${frame.clientWidth}px`;
    frozen.style.height = `${frame.clientHeight}px`;
    frozen.style.right = 'auto';
    frozen.style.bottom = 'auto';
    frozen.querySelectorAll('[id]').forEach((node) => node.removeAttribute('id'));
    const videos = [...body.querySelectorAll('video')];
    const copies = [...frozen.querySelectorAll('video')];
    for (let index = 0; index < copies.length; index++) {
        const source = videos[index];
        const copy = copies[index];
        copy.removeAttribute('autoplay');
        copy.removeAttribute('src');
        copy.preload = 'none';
        copy.querySelectorAll('source').forEach((node) => node.remove());
        const image = document.createElement('canvas');
        image.className = copy.className;
        image.style.cssText = copy.style.cssText;
        image.style.display = 'block';
        image.width = Math.max(1, source.clientWidth);
        image.height = Math.max(1, source.clientHeight);
        image.style.width = `${source.clientWidth}px`;
        image.style.height = `${source.clientHeight}px`;
        const context = image.getContext('2d', { alpha: false });
        if (context) {
            context.fillStyle = '#000';
            context.fillRect(0, 0, image.width, image.height);
            if (source.readyState >= 2 && source.videoWidth && source.videoHeight) {
                const scale = Math.min(
                    image.width / source.videoWidth,
                    image.height / source.videoHeight,
                );
                const width = source.videoWidth * scale;
                const height = source.videoHeight * scale;
                try {
                    context.drawImage(
                        source,
                        (image.width - width) / 2,
                        (image.height - height) / 2,
                        width,
                        height,
                    );
                } catch {
                    /* 刚失去视频帧时保留同尺寸底色。 */
                }
            }
        }
        copy.replaceWith(image);
    }
    frozen.querySelectorAll('audio').forEach((node) => {
        node.removeAttribute('src');
        node.querySelectorAll('source').forEach((source) => source.remove());
        node.remove();
    });
    return frozen;
}
