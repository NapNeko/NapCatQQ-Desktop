export type ThemeImageSources = {
  light: { src: string; srcset: string };
  dark: { src: string; srcset: string };
};

const root = document.documentElement;
let printing = false;
const currentTheme = () => !printing && root.dataset.theme === 'dark' ? 'dark' : 'light';

export function getThemeImageSource(image: HTMLImageElement) {
  const sources = JSON.parse(image.dataset.themeImage!) as ThemeImageSources;
  return sources[currentTheme()];
}

function syncImage(image: HTMLImageElement) {
  const source = getThemeImageSource(image);
  // Update both candidates in one task so a previous theme's srcset cannot win.
  if (image.getAttribute('srcset') !== source.srcset) image.setAttribute('srcset', source.srcset);
  if (image.getAttribute('src') !== source.src) image.setAttribute('src', source.src);
}

export function setThemeImage(image: HTMLImageElement, sources: ThemeImageSources) {
  image.dataset.themeImage = JSON.stringify(sources);
  syncImage(image);
}

function syncImages() {
  document.querySelectorAll<HTMLImageElement>('img[data-theme-image]').forEach(syncImage);
}

syncImages();
new MutationObserver(syncImages).observe(root, { attributes: true, attributeFilter: ['data-theme'] });
window.addEventListener('beforeprint', () => { printing = true; syncImages(); });
window.addEventListener('afterprint', () => { printing = false; syncImages(); });
