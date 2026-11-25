/** Fixed-pitch list window in local list coordinates, end is exclusive. */
export function listWindow(count:number,offset:number,viewport:number,pitch:number,overscan=6){
  if(count<=0||pitch<=0)return {start:0,end:0};
  const top=Math.max(0,offset);
  const bottom=Math.min(count*pitch,Math.max(0,offset+viewport));
  if(bottom<=top)return {start:0,end:0};
  return {start:Math.max(0,Math.floor(top/pitch)-overscan),end:Math.min(count,Math.ceil(bottom/pitch)+overscan)};
}
export function navigationIndex(key:string,current:number,count:number,page:number):number|null{
  if(!count)return null;
  const next=key==="ArrowDown"?current+1:key==="ArrowUp"?current-1:key==="Home"?0:key==="End"?count-1:key==="PageDown"?current+page:key==="PageUp"?current-page:null;
  return next===null?null:Math.max(0,Math.min(count-1,next));
}
export function scrollTarget(index:number,pitch:number,offset:number,viewport:number){
  const start=index*pitch,end=start+pitch;
  if(start<offset)return start;
  if(end>offset+viewport)return end-viewport;
  return offset;
}
