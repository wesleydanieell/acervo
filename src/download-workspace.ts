export type DownloadFilter = "all" | "active" | "ready" | "history";

export function downloadGroup(status: string): Exclude<DownloadFilter, "all"> {
  if (status === "ready") return "ready";
  if (["saved", "failed", "cancelled"].includes(status)) return "history";
  return "active";
}

/** Null requires an explicit selection; the empty string is an intentional root destination. */
export function downloadDestination(
  planned: string,
  selected: string | undefined,
  folders: readonly string[],
): string | null {
  const candidate = selected ?? planned;
  if (candidate === "") return "";
  return folders.includes(candidate) ? candidate : null;
}

/** Mirrors the backend's lexical checks; Rust remains responsible for filesystem safety. */
export function validDownloadPath(path: string): boolean {
  if (!path || new TextEncoder().encode(path).length > 600) return false;
  const parts = path.split("/");
  return parts.length <= 80 && parts.every(part => part.length > 0
    && new TextEncoder().encode(part).length <= 255
    && part === part.trim() && !part.startsWith(".") && part !== "node_modules"
    && !/[\\\p{Cc}]/u.test(part) && !/^99\s*[-–—]\s*cat[aá]logo$/i.test(part));
}

export function planDownloadDestination(
  planned: string, selected: string | undefined, custom: string | undefined, folders: readonly string[],
): { path: string; createMissing: boolean } | null {
  if (custom !== undefined) return validDownloadPath(custom) ? { path: custom, createMissing: true } : null;
  const path = downloadDestination(planned, selected, folders);
  return path === null ? null : { path, createMissing: false };
}


export type CatalogTarget = { root: string; title: string; author: string; destination: string; legacyId?: number | null };
export type DiscoveryRequest = { query: string; language: string; format: string; destination: string; catalogTarget?: CatalogTarget };

export function canImportDownload(root: string, target?: CatalogTarget | null): boolean {
  return Boolean(root) && (!target || target.root === root);
}

export type DownloadCatalogWork = { id: number; title: string; author: string; destination: string; present: boolean };

export function catalogChoice(root: string, work: DownloadCatalogWork): CatalogTarget {
  return { root, title: work.title, author: work.author, destination: work.destination, ...(work.id > 0 ? { legacyId: work.id } : {}) };
}

export function filterDownloadWorks<T extends DownloadCatalogWork>(works: readonly T[], query: string): T[] {
  const fold = (text: string) => text.normalize("NFD").replace(/[\u0300-\u036f]/g, "").toLocaleLowerCase("pt-BR");
  const terms = fold(query.trim()).split(/\s+/).filter(Boolean);
  return works.filter(work => terms.every(term => fold([work.title, work.author, work.destination].join(" ")).includes(term)))
    .sort((a,b) => Number(a.present) - Number(b.present) || a.title.localeCompare(b.title, "pt-BR"));
}


export function sameCatalogChoice(a: CatalogTarget, b: CatalogTarget): boolean {
  return a.root === b.root && a.title === b.title && a.author === b.author &&
    a.destination === b.destination && (a.legacyId ?? null) === (b.legacyId ?? null);
}


export type DownloadEdition = {
  title: string; author: string; language: string; publisher: string; editionLabel: string;
  remoteId?: string; sourceId?: string;
};

export function suggestedEditionSuffix(edition: { language?: string; publisher?: string; editionLabel?: string }): string {
  const parts = [edition.editionLabel, edition.publisher, edition.language]
    .map(value => (value ?? "").replace(/[\p{Cc}]/gu, " ").trim())
    .filter(Boolean);
  const text = [...new Set(parts)].join(" - ");
  let result = "";
  for (const character of text) {
    if (new TextEncoder().encode(result + character).length > 72) break;
    result += character;
  }
  return result.trim().replace(/^[.]+|[.]+$/g,"").trim();
}
