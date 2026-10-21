import { getCollection } from 'astro:content';
import { docsPath, sortDocs } from '../../lib/docs';

export async function GET() {
  const entries = sortDocs(await getCollection('docs'));
  const data = entries.map(entry => ({
    title: entry.data.title,
    description: entry.data.description,
    section: entry.data.section,
    url: docsPath(entry.id),
    text: (entry.body || '').replace(/<!--[\s\S]*?-->/g, '').replace(/!\[[^\]]*\]\([^)]*\)/g, '').replace(/\[([^\]]+)\]\([^)]*\)/g, '$1').replace(/[`#*>|_]/g, '').replace(/\s+/g, ' ').trim(),
  }));
  return new Response(JSON.stringify(data), { headers: { 'Content-Type': 'application/json; charset=utf-8' } });
}
