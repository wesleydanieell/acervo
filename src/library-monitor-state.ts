const KEY="acervo-monitor-generation";
export function nextMonitorGeneration(storage:Pick<Storage,"getItem"|"setItem">=sessionStorage,now=Date.now()):number{
  const stored=Number(storage.getItem(KEY));
  const previous=Number.isSafeInteger(stored)&&stored>=0?stored:0;
  const next=Math.max(now,previous+1);
  storage.setItem(KEY,String(next));
  return next;
}
