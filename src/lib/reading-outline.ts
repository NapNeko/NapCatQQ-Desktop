export type ReadingHeading = { depth: number; slug: string; text: string };
export type ReadingItem = ReadingHeading & { title: string; position: string; section: string; marker: string };

export function readingOutline(headings: ReadingHeading[]): ReadingItem[] {
  const sections = headings.filter(heading => heading.depth === 2);
  const numbered = sections.filter(heading => /^\d+[.、]\s*/.test(heading.text));
  const isSequence = numbered.length > 0 && numbered.every((heading, index) => Number(heading.text.match(/^\d+/)?.[0]) === index + 1);
  let parent: ReadingItem | undefined;
  return headings.filter(heading => heading.depth === 2 || heading.depth === 3).map(heading => {
    if (heading.depth === 3) return { ...heading, title: heading.text, position: parent?.position || '当前章节', section: parent?.slug || heading.slug, marker: '·' };
    const sectionIndex = sections.indexOf(heading);
    const stepIndex = isSequence ? numbered.indexOf(heading) : -1;
    const position = stepIndex >= 0 ? `第 ${stepIndex + 1} / ${numbered.length} 步`
      : isSequence ? (sectionIndex < sections.indexOf(numbered[0]) ? '开始前' : '补充说明')
      : `章节 ${sectionIndex + 1} / ${sections.length}`;
    parent = { ...heading, title: stepIndex >= 0 ? heading.text.replace(/^\d+[.、]\s*/, '') : heading.text, position, section: heading.slug, marker: stepIndex >= 0 ? String(stepIndex + 1).padStart(2, '0') : '·' };
    return parent;
  });
}

export function readingProgress(scrollTop: number, articleTop: number, articleBottom: number, viewportHeight: number, headerOffset: number) {
  const start = articleTop - headerOffset;
  const end = articleBottom - viewportHeight;
  if (end <= start) return scrollTop >= start ? 100 : 0;
  return Math.round(Math.max(0, Math.min(1, (scrollTop - start) / (end - start))) * 100);
}
