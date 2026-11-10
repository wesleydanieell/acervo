/** null means all folders; an empty set means only the library root. */
export function chosenPromptFolders(folders: readonly string[], selected: ReadonlySet<string> | null): string[] {
  return folders.filter((folder) => selected === null || selected.has(folder));
}

export function togglePromptFolder(
  folders: readonly string[],
  selected: ReadonlySet<string> | null,
  folder: string,
): Set<string> {
  const next = new Set(selected === null ? folders : selected);
  if (next.has(folder)) next.delete(folder);
  else if (folders.includes(folder)) next.add(folder);
  return next;
}
