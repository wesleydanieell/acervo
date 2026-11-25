import {useCallback,useLayoutEffect,useRef,type Dispatch,type SetStateAction} from "react";
import {flushSync} from "react-dom";
import {reducedMotion} from "./motion";

type Transition={ready:Promise<void>;finished:Promise<void>;updateCallbackDone:Promise<void>;skipTransition:()=>void};
type TransitionDocument=Document&{startViewTransition?:(update:()=>void)=>Transition};

/** Capture only the settings workspace, where appearance can be selected.
 * All application state remains in the original React tree. */
export function useStyleTransition<T extends string>(style:T,setStyle:Dispatch<SetStateAction<T>>,enabled:boolean){
  const generation=useRef(0);
  const running=useRef<Transition|null>(null);
  const selected=useRef(style);
  const requested=useRef(style);
  selected.current=style;
  useLayoutEffect(()=>{
    const preference=window.matchMedia("(prefers-reduced-motion: reduce)");
    const skip=()=>{running.current?.skipTransition();};
    const reduce=()=>{if(preference.matches)skip();};
    const hide=()=>{if(document.hidden)skip();};
    preference.addEventListener("change",reduce);
    document.addEventListener("visibilitychange",hide);
    if(!enabled)skip();
    return()=>{
      preference.removeEventListener("change",reduce);
      document.removeEventListener("visibilitychange",hide);
      skip();
    };
  },[enabled]);
  return useCallback((next:T)=>{
    if(next===requested.current&&next===selected.current)return;
    requested.current=next;
    const token=++generation.current;
    running.current?.skipTransition();
    running.current=null;
    const root=document.documentElement;
    const doc=document as TransitionDocument;
    const cleanup=()=>{
      if(token!==generation.current)return;
      delete root.dataset.styleTransition;
      running.current=null;
    };
    if(!enabled||reducedMotion()||document.hidden||typeof doc.startViewTransition!=="function"){
      cleanup();setStyle(next);return;
    }
    // Named elements must be present in both snapshots. The attribute
    // also prevents fallback animations from competing with the browser.
    root.dataset.styleTransition=next;
    try{
      const transition=doc.startViewTransition(()=>{
        if(token===generation.current)flushSync(()=>setStyle(next));
      });
      running.current=transition;
      void transition.ready.catch(()=>{});
      void transition.updateCallbackDone.catch(()=>{
        if(token===generation.current)setStyle(next);
      });
      void transition.finished.then(cleanup,cleanup);
    }catch{
      cleanup();setStyle(next);
    }
  },[enabled,setStyle]);
}
