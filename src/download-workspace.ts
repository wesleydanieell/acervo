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
