/** null means the complete library is selected; an empty set means root only. */
export function chosenPromptFolders(folders: readonly string[], selected: ReadonlySet<string> | null): string[] {
  return folders.filter((folder) => selected === null || selected.has(folder));
}

export function directPromptChildren(folders: readonly string[], parent: string): string[] {
  return folders.filter((folder) => {
    const suffix = parent ? (folder.startsWith(parent + "/") ? folder.slice(parent.length + 1) : "") : folder;
    return Boolean(suffix) && !suffix.includes("/");
  });
}

export function promptSubtree(folders: readonly string[], parent: string): string[] {
  if (!parent) return [...folders];
  return folders.filter((folder) => folder === parent || folder.startsWith(parent + "/"));
}

export type FolderSelection = "all" | "partial" | "none";

export function promptFolderSelection(
  folders: readonly string[],
  selected: ReadonlySet<string> | null,
  parent: string,
): FolderSelection {
  const members = promptSubtree(folders, parent);
  if (!members.length) return "none";
  if (selected === null) return "all";
  const count = members.filter((folder) => selected.has(folder)).length;
  return count === 0 ? "none" : count === members.length ? "all" : "partial";
}

/** Toggle a folder and every descendant. Root toggles the whole library. */
export function togglePromptFolder(
  folders: readonly string[],
  selected: ReadonlySet<string> | null,
  folder: string,
): Set<string> | null {
  if (!folder) return promptFolderSelection(folders, selected, "") === "all" ? new Set() : null;
  const descendants = promptSubtree(folders, folder);
  const next = new Set(selected === null ? folders : selected);
  if (!descendants.length) return next;
  const allIncluded = descendants.every((entry) => next.has(entry));
  for (const entry of descendants) {
    if (allIncluded) next.delete(entry);
    else next.add(entry);
  }
  return next;
}
