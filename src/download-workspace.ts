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
