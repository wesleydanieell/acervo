/** Range selection follows the same visible order as the current file explorer. */
export function selectVisibleRange(
  visiblePaths: readonly string[],
  anchor: string | null,
  target: string,
  selected: ReadonlySet<string>,
  extend: boolean,
): Set<string> {
  const end = visiblePaths.indexOf(target);
  if (end < 0) return new Set(selected);
  const start = anchor === null ? -1 : visiblePaths.indexOf(anchor);
  const next = extend ? new Set(selected) : new Set<string>();
  if (start < 0) {
    next.add(target);
    return next;
  }
  for (let index = Math.min(start, end); index <= Math.max(start, end); index++) {
    next.add(visiblePaths[index]);
  }
  return next;
}
