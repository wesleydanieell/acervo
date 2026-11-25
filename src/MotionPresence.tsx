import {cloneElement,useLayoutEffect,useRef,useState,type ReactElement} from "react";
import {motion,reducedMotion} from "./motion";
type PresenceProps={className?:string;inert?:boolean;"aria-hidden"?:boolean};
/** Visual exit only. The retained subtree is immediately inert; no action or
 * focus can be dispatched to controls while their exit animation plays. */
export function MotionPresence({children}:{children:ReactElement<PresenceProps>|null|false|undefined}){
  const last=useRef<ReactElement<PresenceProps>|null>(null);
  const [,invalidate]=useState(0);
  const visible=Boolean(children);
  useLayoutEffect(()=>{if(children)last.current=children;},[children]);
  useLayoutEffect(()=>{
    if(visible||!last.current)return;
    const finish=()=>{last.current=null;invalidate(n=>n+1);};
    if(reducedMotion()||document.hidden){finish();return;}
    const timer=window.setTimeout(finish,motion.exit);
    const preference=window.matchMedia("(prefers-reduced-motion: reduce)");
    const reduce=()=>{if(preference.matches)finish();};
    preference.addEventListener("change",reduce);
    return()=>{window.clearTimeout(timer);preference.removeEventListener("change",reduce);};
  },[visible]);
  if(children)return children;
  if(!last.current)return null;
  return cloneElement(last.current,{className:(last.current.props.className??"")+" motion-exiting",inert:true,"aria-hidden":true});
}
