// Keep the toolbar outside the code scroller so long commands never hide Copy.
const languageNames: Record<string, string> = {
  bash: 'Bash',
  sh: 'Shell',
  shell: 'Shell',
  powershell: 'PowerShell',
  ps1: 'PowerShell',
  text: '纯文本',
  plaintext: '纯文本',
  json: 'JSON',
  yaml: 'YAML',
  yml: 'YAML',
  toml: 'TOML',
  javascript: 'JavaScript',
  typescript: 'TypeScript',
};

document.querySelectorAll<HTMLPreElement>('.doc-prose pre').forEach(pre => {
  const code = pre.querySelector('code');
  if (!code || pre.closest('.doc-code-block')) return;

  const language = pre.dataset.language || code.className.match(/language-([\w-]+)/)?.[1] || 'text';
  const label = languageNames[language] || language;
  const block = document.createElement('div');
  block.className = 'doc-code-block';
  const toolbar = document.createElement('div');
  toolbar.className = 'doc-code-toolbar';
  const caption = document.createElement('span');
  caption.className = 'doc-code-language';
  const glyph = document.createElement('span');
  glyph.className = 'doc-code-glyph';
  glyph.setAttribute('aria-hidden', 'true');
  glyph.textContent = language === 'text' || language === 'plaintext' ? '≡' : '>_';
  caption.append(glyph, document.createTextNode(label));

  const button = document.createElement('button');
  button.type = 'button';
  button.className = 'doc-code-copy';
  button.textContent = '复制';
  button.setAttribute('aria-label', `复制 ${label} 代码`);
  const feedback = document.createElement('span');
  feedback.className = 'doc-code-feedback';
  feedback.setAttribute('role', 'status');
  feedback.setAttribute('aria-live', 'polite');
  feedback.setAttribute('aria-atomic', 'true');

  let feedbackTimer: ReturnType<typeof setTimeout> | undefined;
  button.addEventListener('click', async () => {
    clearTimeout(feedbackTimer);
    button.disabled = true;
    feedback.textContent = '';
    block.dataset.copyState = 'pending';
    try {
      await navigator.clipboard.writeText(code.textContent || '');
      button.textContent = '已复制';
      feedback.textContent = '代码已复制';
      block.dataset.copyState = 'success';
    } catch {
      button.textContent = '重试';
      feedback.textContent = '复制失败，请选中代码手动复制';
      block.dataset.copyState = 'error';
    } finally {
      button.disabled = false;
      feedbackTimer = setTimeout(() => {
        button.textContent = '复制';
        feedback.textContent = '';
        delete block.dataset.copyState;
      }, 4000);
    }
  });

  toolbar.append(caption, button);
  pre.before(block);
  pre.tabIndex = 0;
  pre.setAttribute('role', 'region');
  pre.setAttribute('aria-label', `${label} 代码，可横向滚动`);
  block.append(toolbar, pre, feedback);
});
