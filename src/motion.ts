export const motion={fast:140,standard:220,emphasized:340,exit:180,ease:"cubic-bezier(.2,.7,.18,1)"} as const;
export const reducedMotion=()=>window.matchMedia("(prefers-reduced-motion: reduce)").matches;
export type Rect={left:number;top:number;width:number;height:number};
export function morphTransform(before:Rect,after:Rect):string|null{
  if(![before.left,before.top,before.width,before.height,after.left,after.top,after.width,after.height].every(Number.isFinite)||before.width<=0||before.height<=0||after.width<=0||after.height<=0)return null;
  const x=before.left-after.left,y=before.top-after.top;
  const sx=before.width/after.width,sy=before.height/after.height;
  if(Math.abs(x)<.5&&Math.abs(y)<.5&&Math.abs(sx-1)<.005&&Math.abs(sy-1)<.005)return null;
  return `translate(${x}px,${y}px) scale(${sx},${sy})`;
}
export function animateElement(element:Element|null,keyframes:Keyframe[],duration:number=motion.standard):Animation|null{
  if(!element||reducedMotion()||document.hidden||typeof element.animate!=="function")return null;
  try{return element.animate(keyframes,{duration,easing:motion.ease});}catch{return null;}
}
