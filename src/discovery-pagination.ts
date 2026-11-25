export type SourceResult = {
  sourceId: string;
  sourceName: string;
  title: string;
  author: string;
  format: string;
  language: string;
  downloadUrl: string;
  remoteId?: string;
};

export type SearchOutcome = {
  results: SourceResult[];
  errors: string[];
  nextPages?: Record<string, string>;
};

export function resultIdentity(result: SourceResult): string {
  return JSON.stringify(result.remoteId
    ? [result.sourceId, result.remoteId, result.format, result.downloadUrl]
    : [result.sourceId, result.title, result.author, result.language, result.format, result.downloadUrl]);
}

export function mergeSourcePage(
  current: SearchOutcome,
  incoming: SearchOutcome,
  sourceId: string,
): SearchOutcome {
  const errors = [...current.errors, ...incoming.errors];
  // A failed page must leave both the results and the requested cursor retryable.
  if (incoming.errors.length) return { ...current, errors };

  const seen = new Set<string>();
  const results = [...current.results, ...incoming.results.filter(result => result.sourceId === sourceId)]
    .filter(result => {
      const identity = resultIdentity(result);
      if (seen.has(identity)) return false;
      seen.add(identity);
      return true;
    });
  const nextPages = { ...current.nextPages };
  const cursor = Object.prototype.hasOwnProperty.call(incoming.nextPages ?? {}, sourceId)
    ? incoming.nextPages![sourceId] : undefined;
  if (cursor && cursor !== current.nextPages?.[sourceId]) nextPages[sourceId] = cursor;
  else delete nextPages[sourceId];
  return { results, errors, nextPages };
}
