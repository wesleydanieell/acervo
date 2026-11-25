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

export type SelectionOperation="add"|"remove"|"invert";
export function updateSelection(selected:ReadonlySet<string>,paths:readonly string[],operation:SelectionOperation):Set<string>{
  const next=new Set(selected);
  for(const path of new Set(paths)){
    if(operation==="remove"||(operation==="invert"&&next.has(path)))next.delete(path);
    else next.add(path);
  }
  return next;
}
export function selectionSummary(selected:ReadonlySet<string>,results:readonly string[]){
  let inResults=0;
  for(const path of new Set(results))if(selected.has(path))inResults++;
  return {total:selected.size,inResults,outside:selected.size-inResults};
}
export function nextSelectionBatch(selected:ReadonlySet<string>,ordered:readonly string[],limit=100){
  const batch:string[]=[];
  for(const path of ordered)if(selected.has(path)&&batch.length<limit)batch.push(path);
  return [...new Set(batch)];
}
export function folderContains(folder:string,child:string,recursive:boolean){
  return child===folder||(recursive&&(!folder||child.startsWith(folder+"/")));
}
