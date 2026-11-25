import {useLayoutEffect,useRef} from "react";
import {animateElement,morphTransform,motion,reducedMotion,type Rect} from "./motion";
type Snapshot={key:string;rect:Rect;radius:string;cover:HTMLElement;source:HTMLElement};
const visible=(rect:DOMRect,clip:DOMRect)=>rect.width>0&&rect.height>0&&rect.left>=Math.max(0,clip.left)&&rect.top>=Math.max(0,clip.top)&&rect.right<=Math.min(window.innerWidth,clip.right)&&rect.bottom<=Math.min(window.innerHeight,clip.bottom);

/** A single, non-interactive visual clone makes the cover travel to its
 * destination without retaining book rows or moving their real DOM nodes. */
export function useBookCoverMotion(selectionKey:string|undefined,root:string){
  const pending=useRef<Snapshot|null>(null);
  const prepare=(key:string,card:HTMLElement)=>{
    pending.current=null;
    if(reducedMotion()||document.hidden)return;
    const cover=card.querySelector<HTMLElement>(".book-cover");
    const main=card.closest(".main-area");
    if(!cover||!main)return;
    const rect=cover.getBoundingClientRect();
    if(!visible(rect,main.getBoundingClientRect()))return;
    const clone=cover.cloneNode(true) as HTMLElement;
    clone.removeAttribute("id");
    clone.setAttribute("aria-hidden","true");
    clone.inert=true;
    clone.classList.add("shared-cover-flight");
    // The real image is already decoded/cached. Don't leave a lazy image
    // waiting for an observer on the flight layer.
    const image=clone.querySelector("img");
    if(image)image.loading="eager";
    const style=getComputedStyle(cover);
    clone.style.background=style.background;
    clone.style.color=style.color;
    clone.style.boxShadow=style.boxShadow;
    pending.current={key,rect,radius:style.borderRadius,cover:clone,source:cover};
  };
  useLayoutEffect(()=>{
    const snapshot=pending.current;
    pending.current=null;
    if(!snapshot||snapshot.key!==selectionKey||reducedMotion()||document.hidden)return;
    const drawer=document.querySelector<HTMLElement>(".drawer-backdrop:not(.motion-exiting)");
    const target=drawer?.querySelector<HTMLElement>(".details-cover .book-cover");
    if(!drawer||!target)return;
    // The cover itself travels; the containing drawer should not translate
    // beneath its destination. Keep the ordinary fade and exit animations.
    drawer.dataset.sharedCover="true";
    const rect=target.getBoundingClientRect();
    const transform=morphTransform(snapshot.rect,rect);
    if(!transform||rect.width<=0||rect.height<=0||rect.top<0||rect.bottom>window.innerHeight){
      delete drawer.dataset.sharedCover;return;
    }
    const clone=snapshot.cover;
    const radius=getComputedStyle(target).borderRadius;
    Object.assign(clone.style,{left:rect.left+"px",top:rect.top+"px",width:rect.width+"px",height:rect.height+"px",borderRadius:radius});
    document.body.append(clone);
    target.classList.add("shared-cover-target");
    snapshot.source.classList.add("shared-cover-target");
    const animation=animateElement(clone,[{transform,borderRadius:snapshot.radius},{transform:"none",borderRadius:radius}],motion.emphasized+80);
    let finished=false;
    const finish=()=>{
      if(finished)return;
      finished=true;
      animation?.cancel();clone.remove();
      target.classList.remove("shared-cover-target");
      snapshot.source.classList.remove("shared-cover-target");
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
  return prepare;
}
