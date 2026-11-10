/** Conservative, accent-insensitive matching of alternate files already in the local library.
 * These are suggestions only: never change the catalog's present/missing status automatically.
 */
export type EditionFile = { path: string; title: string; format: string };
export type EditionWork = { title: string; associatedFiles: string[] };

export function titleKey(title: string): string {
  return title.normalize("NFKD").replace(/[\u0300-\u036f]/g, "")
    .toLowerCase().replace(/[^a-z0-9]+/g, " ").trim().replace(/\s+/g, " ");
}

export function indexEditions<T extends EditionFile>(files: T[]): Map<string, T[]> {
  const byTitle = new Map<string, T[]>();
  for (const file of files) {
    const key = titleKey(file.title);
    if (key.length < 4) continue; // Prevent ambiguous short titles.
    const list = byTitle.get(key) || [];
    list.push(file);
    byTitle.set(key, list);
  }
  return byTitle;
}

export function alternateEditions<T extends EditionFile>(
  work: EditionWork, index: Map<string, T[]>,
): T[] {
  const key = titleKey(work.title);
  if (key.length < 4) return [];
  const candidates = index.get(key) || [];
  const assigned = new Set(work.associatedFiles);
  const associatedFormats = new Set(candidates.filter((file) => assigned.has(file.path)).map((file) => file.format));
  return candidates.filter((file) => !assigned.has(file.path) && !associatedFormats.has(file.format));
}
