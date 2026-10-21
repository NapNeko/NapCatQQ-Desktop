// Keep authored URLs independent of SITE_BASE; enrich paired product captures.
export default function docsMarkup() {
  const prefix = (process.env.SITE_BASE || '/').replace(/^\/+|\/+$/g, '');
  const base = prefix ? `/${prefix}/` : '/';
  const local = (url) => typeof url === 'string' && url.startsWith('/docs/') ? `${base}${url.slice(1)}` : url;
  const el = (tagName, properties, children = []) => ({ type: 'element', tagName, properties, children });
  const txt = (value) => ({ type: 'text', value });
  return {
    name: 'docs-links-and-screenshots',
    element: { filter: ['a', 'img'], visit(original) {
      const node = { ...original, properties: { ...original.properties } };
      if (node.type === 'element') {
        if (node.properties?.href) node.properties.href = local(node.properties.href);
        if (node.tagName === 'img') {
          const { src, alt = '' } = node.properties;
          node.properties.src = local(src);
          node.properties.loading = 'lazy';
          node.properties.decoding = 'async';
          if (typeof src === 'string' && src.includes('/docs/screenshots/') && src.endsWith('-light.png')) {
            const light = local(src), dark = light.replace(/-light\.png$/, '-dark.png');
            return el('span', { className: ['doc-shot'], 'data-shot': '', 'data-mode': 'auto' }, [
              el('button', { type: 'button', className: ['shot-open'], 'data-shot-open': '', 'aria-label': `放大：${alt}`, 'aria-haspopup': 'dialog' }, [
                el('img', { src: light, alt, width: 1440, height: 1000, loading: 'lazy', decoding: 'async', className: ['shot-light'] }),
                el('img', { src: dark, alt, width: 1440, height: 1000, loading: 'lazy', decoding: 'async', className: ['shot-dark'] }),
              ]),
              el('span', { className: ['shot-caption'] }, [
                el('span', {}, [txt(`${alt} · 演示数据`)]),
                el('span', { className: ['shot-switch'], role: 'group', 'aria-label': '截图主题' }, ['light', 'dark'].map(mode => el('button', { type: 'button', 'data-shot-theme': mode, 'aria-pressed': 'false' }, [txt(mode === 'light' ? '浅色' : '深色')]))),
              ]),
            ]);
          }
        }
      }
      return node;
    } },
  };
}
