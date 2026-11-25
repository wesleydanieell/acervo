import {useLayoutEffect,useRef,type RefObject} from "react";
import {animateElement,morphTransform,motion,reducedMotion,type Rect} from "./motion";
type Capture={key:string;rect:Rect};
const MAX=16;
export function useLayoutMotion(layout:string,ref:RefObject<HTMLElement|null>){
  const capture=useRef<Capture[]>([]);
  const running=useRef<Animation[]>([]);
  const requested=useRef(false);
  const prepare=()=>{
    for(const animation of running.current)animation.cancel();
    running.current=[];
    capture.current=[];
    requested.current=true;
    if(reducedMotion()||document.hidden)return;
    const main=ref.current;if(!main)return;
    // Only sample the first visible viewport near the top. For a distant
    // scroll position prefer a simple reveal instead of a giant FLIP pass.
    if(main.scrollTop>main.clientHeight)return;
    const bounds=main.getBoundingClientRect();
    const nodes=main.querySelectorAll<HTMLElement>(".book-card[data-book-key]");
    for(let i=0;i<Math.min(nodes.length,100)&&capture.current.length<MAX;i++){
      const card=nodes[i],cover=card.querySelector(".book-cover");
      if(!cover||!card.dataset.bookKey)continue;
      const rect=cover.getBoundingClientRect();
      if(rect.bottom>bounds.top&&rect.top<bounds.bottom&&rect.width>0&&rect.height>0)capture.current.push({key:card.dataset.bookKey,rect});
    }
  };
  useLayoutEffect(()=>{
    const main=ref.current,old=capture.current;
    capture.current=[];
    const switching=requested.current;requested.current=false;
    if(!main||!switching||reducedMotion()||document.hidden)return;
    const byKey=new Map(old.map(item=>[item.key,item.rect]));
    const list=main.querySelector(".books-list,.books-grid");
    const reveal=animateElement(list,[{opacity:.72},{opacity:1}],motion.standard);
    if(reveal)running.current.push(reveal);
    const nodes=main.querySelectorAll<HTMLElement>(".book-card[data-book-key]");
    for(let i=0;i<Math.min(nodes.length,100)&&byKey.size;i++){
      const card=nodes[i],key=card.dataset.bookKey??"",before=byKey.get(key);
      if(!before)continue;
      byKey.delete(key);
      const cover=card.querySelector(".book-cover");
      if(!cover)continue;
      const after=cover.getBoundingClientRect();
      // Limit distance and scale; never animate a cover across an entire
      // library or mount off-screen rows solely for a transition.
      if(Math.abs(before.top-after.top)>main.clientHeight||Math.abs(before.left-after.left)>main.clientWidth)continue;
      const transform=morphTransform(before,after);
      if(!transform)continue;
      const animation=animateElement(cover,[{transform,transformOrigin:"top left"},{transform:"none",transformOrigin:"top left"}],motion.emphasized);
      if(animation)running.current.push(animation);
    }
    const preference=window.matchMedia("(prefers-reduced-motion: reduce)");
    const stop=()=>{for(const animation of running.current)animation.cancel();running.current=[];};
    const reduce=()=>{if(preference.matches)stop();};
    preference.addEventListener("change",reduce);
    return()=>{preference.removeEventListener("change",reduce);stop();};
  },[layout,ref]);
  return prepare;
}
