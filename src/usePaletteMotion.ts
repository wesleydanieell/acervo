import {useLayoutEffect,useRef} from "react";
import {interpolatePalette,paletteEqual,paletteKeys,resolvePalette,type PaletteName,type ResolvedPalette,type ThemeTokens} from "./palette-motion";
const DURATION=320;
let unlockFrame=0;
// Force the new color mode to be computed before allowing local control
// transitions again, otherwise text can jump before its background.
function atomicPaint(value:ResolvedPalette){
  const root=document.documentElement;
  cancelAnimationFrame(unlockFrame);
  root.dataset.paletteAtomic="true";
  paint(value);
  // Flush styles before releasing the transition suppression next frame.
  void root.offsetWidth;
  unlockFrame=requestAnimationFrame(()=>{delete root.dataset.paletteAtomic;});
}
function paint(value:ResolvedPalette){
  const root=document.documentElement;
  root.dataset.colorMode=value.mode;
  for(const key of paletteKeys)root.style.setProperty(key,value.colors[key]);
}
export function usePaletteMotion(name:PaletteName,theme:ThemeTokens|null){
  const current=useRef<ResolvedPalette|null>(null);
  useLayoutEffect(()=>{
    const target=resolvePalette(name,theme),start=current.current;
    const preference=window.matchMedia("(prefers-reduced-motion: reduce)");
    let frame=0;
    const finish=()=>{cancelAnimationFrame(frame);current.current=target;paint(target);};
    if(!start||start.mode!==target.mode||preference.matches){
      current.current=target;atomicPaint(target);return;
    }
    if(paletteEqual(start,target)){finish();return;}
    const began=performance.now();
    const step=(now:number)=>{
      if(preference.matches||document.hidden){finish();return;}
      const t=Math.min(1,Math.max(0,(now-began)/DURATION));
      const value=interpolatePalette(start,target,1-(1-t)**3);
      current.current=value;paint(value);
      if(t<1)frame=requestAnimationFrame(step);
    };
    const reduce=()=>{if(preference.matches)finish();};
    const visibility=()=>{if(document.hidden)finish();};
    preference.addEventListener("change",reduce);document.addEventListener("visibilitychange",visibility);
    frame=requestAnimationFrame(step);
    return()=>{cancelAnimationFrame(frame);preference.removeEventListener("change",reduce);document.removeEventListener("visibilitychange",visibility);};
  },[name,theme]);
}
