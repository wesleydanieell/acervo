import { type SourceResult, resultIdentity } from "./discovery-pagination";

export type DiscoveryWork = {
  key: string;
  title: string;
  author: string;
  sourceId: string;
  sourceName: string;
  editions: SourceResult[];
  metadata: SourceResult[];
};

const downloadFormats = new Set(["pdf", "epub", "mobi", "azw3", "djvu"]);

export function groupDiscoveryWorks(results: readonly SourceResult[]): DiscoveryWork[] {
  const works = new Map<string, DiscoveryWork>();
  const seen = new Set<string>();

  for (const result of results) {
    const key = result.remoteId
      ? JSON.stringify(["remote", result.sourceId, result.remoteId, result.title, result.author])
      : JSON.stringify(["result", resultIdentity(result)]);
    // Only deduplicate identical editions and metadata within a work.
    const identity = JSON.stringify([
      key, resultIdentity(result), result.sourceName, result.title,
      result.author, result.language,
    ]);
    if (seen.has(identity)) continue;
    seen.add(identity);

    let work = works.get(key);
    if (!work) {
      work = {
        key, title: result.title, author: result.author,
        sourceId: result.sourceId, sourceName: result.sourceName,
        editions: [], metadata: [],
      };
      works.set(key, work);
    }
    const target = result.downloadUrl && downloadFormats.has(result.format.toLowerCase())
      ? work.editions : work.metadata;
    target.push(result);
  }

  return [...works.values()];
}
