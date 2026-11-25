import {useLayoutEffect,useRef,type RefObject} from "react";
import {animateElement,morphTransform,motion,reducedMotion,type Rect} from "./motion";

export function useAppearanceMotion(style:string){
  const previous=useRef<string|null>(null);
  useLayoutEffect(()=>{
    const root=document.documentElement;
    const old=previous.current;
    previous.current=style;
    localStorage.setItem("acervo-workspace-style",style);
    if(old===null||old===style||reducedMotion()||document.hidden){root.dataset.visualStyle=style;return;}
    // Only the two chrome surfaces morph. Avoid snapshots of a large book grid,
    // avoid animating layout properties and never change the hit target.
    const elements=Array.from(document.querySelectorAll<HTMLElement>(".sidebar,.topbar")).filter(e=>e.getBoundingClientRect().width>0);
    const before=elements.map(element=>({rect:element.getBoundingClientRect(),radius:getComputedStyle(element).borderRadius}));
    root.dataset.visualStyle=style;
    const animations:Animation[]=[];
    elements.forEach((element,index)=>{
      const after=element.getBoundingClientRect(),oldRect=before[index].rect;
      const transform=morphTransform(oldRect,after);
      // Don't translate a mobile drawer or scale text across very different sizes.
      const safe=transform&&Math.abs(oldRect.width-after.width)<40&&Math.abs(oldRect.height-after.height)<40&&window.innerWidth>860;
      const animation=animateElement(element,[{borderRadius:before[index].radius,...(safe?{transform,transformOrigin:"top left"}:{opacity:.8})},{borderRadius:getComputedStyle(element).borderRadius,...(safe?{transform:"none",transformOrigin:"top left"}:{opacity:1})}],motion.emphasized);
      if(animation)animations.push(animation);
    });
    const main=animateElement(document.querySelector(".main-area"),[{opacity:.78},{opacity:1}],motion.standard);
    if(main)animations.push(main);
    const preference=window.matchMedia("(prefers-reduced-motion: reduce)");
    const stop=()=>{if(preference.matches)animations.forEach(a=>a.cancel());};
    preference.addEventListener("change",stop);
    return()=>{preference.removeEventListener("change",stop);animations.forEach(a=>a.cancel());};
  },[style]);
}
export function usePageMotion(ref:RefObject<HTMLElement|null>,key:string){
  const last=useRef<string|null>(null);
  useLayoutEffect(()=>{
    const old=last.current;last.current=key;
    if(old===null||old===key)return;
    const main=ref.current;if(!main)return;
    main.scrollTop=0;
    const head=main.querySelector(".page-intro,.content-top");
    const animations=[animateElement(main,[{opacity:.68},{opacity:1}],motion.standard),
      animateElement(head,[{transform:"translateY(8px)",opacity:.75},{transform:"none",opacity:1}],motion.emphasized)];
    const preference=window.matchMedia("(prefers-reduced-motion: reduce)");
    const stop=()=>{if(preference.matches)animations.forEach(a=>a?.cancel());};
    preference.addEventListener("change",stop);
    return()=>{preference.removeEventListener("change",stop);animations.forEach(a=>a?.cancel());};
  },[key,ref]);
}

/** Shared selection morph. The real selected control updates immediately;
 * a non-interactive accent outline travels from the previous selection. */
export function useSelectionMotion(selector:string,key:string){
  const previous=useRef<{rect:Rect;radius:string}|null>(null);
  useLayoutEffect(()=>{
    const target=document.querySelector<HTMLElement>(selector);
    if(!target){previous.current=null;return;}
    const rect=target.getBoundingClientRect();
    const radius=getComputedStyle(target).borderRadius;
    const before=previous.current;
    previous.current={rect,radius};
    if(!before||reducedMotion()||document.hidden||rect.width<=0||rect.height<=0)return;
    const transform=morphTransform(before.rect,rect);
    if(!transform||rect.bottom<0||rect.top>window.innerHeight||rect.right<0||rect.left>window.innerWidth)return;
    if(Math.abs(before.rect.top-rect.top)>window.innerHeight*.7)return;
    const highlight=document.createElement("span");
    highlight.className="motion-selection-trace";
    highlight.setAttribute("aria-hidden","true");
    Object.assign(highlight.style,{left:rect.left+"px",top:rect.top+"px",width:rect.width+"px",height:rect.height+"px",borderRadius:radius});
    document.body.append(highlight);
    const animation=animateElement(highlight,[{transform,opacity:.8,borderRadius:before.radius},{transform:"none",opacity:0,borderRadius:radius}],motion.emphasized);
    const finish=()=>highlight.remove();
    if(!animation){finish();return;}
    animation.finished.then(finish,finish);
    const preference=window.matchMedia("(prefers-reduced-motion: reduce)");
    const stop=()=>{animation.cancel();finish();};
    const reduce=()=>{if(preference.matches)stop();};
    preference.addEventListener("change",reduce);window.addEventListener("resize",stop);
    return()=>{preference.removeEventListener("change",reduce);window.removeEventListener("resize",stop);stop();};
  },[selector,key]);
}
