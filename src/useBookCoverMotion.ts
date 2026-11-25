import {useLayoutEffect,useRef,useState} from "react";
import {animateElement,morphTransform,motion,reducedMotion,type Rect} from "./motion";
type Snapshot={key:string;root:string;direction:"open"|"close";rect:Rect;radius:string;cover:HTMLElement;source:HTMLElement};
const visible=(rect:DOMRect,clip:DOMRect)=>rect.width>0&&rect.height>0&&rect.left>=Math.max(0,clip.left)&&rect.top>=Math.max(0,clip.top)&&rect.right<=Math.min(window.innerWidth,clip.right)&&rect.bottom<=Math.min(window.innerHeight,clip.bottom);
function cardCover(key:string){
  return document.querySelector<HTMLElement>('.main-area .book-card[data-book-key="'+CSS.escape(key)+'"] .book-cover');
}
function snapshot(key:string,root:string,direction:Snapshot["direction"],cover:HTMLElement):Snapshot{
  const clone=cover.cloneNode(true) as HTMLElement;
  clone.removeAttribute("id");
  clone.setAttribute("aria-hidden","true");
  clone.inert=true;
  clone.classList.remove("shared-cover-target");
  clone.classList.add("shared-cover-flight");
  const image=clone.querySelector("img");
  if(image)image.loading="eager";
  const style=getComputedStyle(cover);
  clone.style.background=style.background;
  clone.style.color=style.color;
  clone.style.boxShadow=style.boxShadow;
  return {key,root,direction,rect:cover.getBoundingClientRect(),radius:style.borderRadius,cover:clone,source:cover};
}

/** A single inert visual clone travels between the library and the details.
 * A closing dock releases its layout before measuring the return destination.
 * Off-screen or unmounted rows always use the ordinary drawer exit. */
export function useBookCoverMotion(selectionKey:string|undefined,root:string){
  const pending=useRef<Snapshot|null>(null);
  const active=useRef<{key:string;clone:HTMLElement}|null>(null);
  const [immediateExit,setImmediateExit]=useState(false);
  const prepare=(key:string,card:HTMLElement)=>{
    pending.current=null;
    setImmediateExit(false);
    if(key===selectionKey||reducedMotion()||document.hidden)return;
    const cover=card.querySelector<HTMLElement>(".book-cover");
    const main=card.closest(".main-area");
    if(!cover||!main||!visible(cover.getBoundingClientRect(),main.getBoundingClientRect()))return;
    pending.current=snapshot(key,root,"open",cover);
  };
  const prepareReturn=()=>{
    pending.current=null;
    setImmediateExit(false);
    if(!selectionKey||reducedMotion()||document.hidden)return;
    const drawer=document.querySelector<HTMLElement>(".drawer-backdrop:not(.motion-exiting)");
    const original=drawer?.querySelector<HTMLElement>(".details-cover .book-cover");
    const destination=cardCover(selectionKey);
    const main=destination?.closest(".main-area");
    if(!drawer||!original||!destination||!main||!visible(destination.getBoundingClientRect(),main.getBoundingClientRect()))return;
    // Reverse from the current flight position if the user closes mid-open.
    const source=active.current?.key===selectionKey?active.current.clone:original;
    if(!visible(source.getBoundingClientRect(),document.documentElement.getBoundingClientRect()))return;
    pending.current=snapshot(selectionKey,root,"close",source);
    if(getComputedStyle(drawer).position==="relative")setImmediateExit(true);
  };
  useLayoutEffect(()=>{
    const shot=pending.current;
    pending.current=null;
    if(immediateExit)setImmediateExit(false);
    if(!shot||shot.root!==root||reducedMotion()||document.hidden)return;
    const opening=shot.direction==="open";
    if(opening?shot.key!==selectionKey:!!selectionKey)return;
    const drawer=document.querySelector<HTMLElement>(".drawer-backdrop:not(.motion-exiting)");
    const target=opening?drawer?.querySelector<HTMLElement>(".details-cover .book-cover"):cardCover(shot.key);
    if(!target)return;
    if(opening&&drawer)drawer.dataset.sharedCover="true";
    const rect=target.getBoundingClientRect();
    const clip=opening?document.documentElement:target.closest(".main-area");
    const transform=morphTransform(shot.rect,rect);
    if(!transform||!clip||!visible(rect,clip.getBoundingClientRect())){
      if(opening&&drawer)delete drawer.dataset.sharedCover;return;
    }
    const clone=shot.cover;
    const radius=getComputedStyle(target).borderRadius;
    Object.assign(clone.style,{left:rect.left+"px",top:rect.top+"px",width:rect.width+"px",height:rect.height+"px",borderRadius:radius});
    document.body.append(clone);
    active.current={key:shot.key,clone};
    target.classList.add("shared-cover-target");
    shot.source.classList.add("shared-cover-target");
    // On a rapid close the captured source was the previous flight, not the
    // real drawer cover. Hide the retained exiting cover as well.
    const exiting=opening?null:document.querySelector<HTMLElement>(".drawer-backdrop.motion-exiting .details-cover .book-cover");
    exiting?.classList.add("shared-cover-target");
    const animation=animateElement(clone,[{transform,borderRadius:shot.radius},{transform:"none",borderRadius:radius}],opening?motion.emphasized+80:motion.emphasized);
    let finished=false;
    const finish=()=>{
      if(finished)return;
      finished=true;
      animation?.cancel();clone.remove();
      if(active.current?.clone===clone)active.current=null;
      target.classList.remove("shared-cover-target");
      shot.source.classList.remove("shared-cover-target");
      exiting?.classList.remove("shared-cover-target");
      window.removeEventListener("resize",finish);
      window.removeEventListener("scroll",finish,true);
      document.removeEventListener("visibilitychange",hide);
      preference.removeEventListener("change",reduce);
    };
    const preference=window.matchMedia("(prefers-reduced-motion: reduce)");
    const reduce=()=>{if(preference.matches)finish();};
    const hide=()=>{if(document.hidden)finish();};
    if(!animation){finish();return;}
    window.addEventListener("resize",finish);
    window.addEventListener("scroll",finish,true);
    preference.addEventListener("change",reduce);
    document.addEventListener("visibilitychange",hide);
    void animation.finished.then(finish,finish);
    return finish;
  },[selectionKey,root]);
  return {prepare,prepareReturn,immediateExit};
}
