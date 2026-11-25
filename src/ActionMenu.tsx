import {useEffect,useId,useRef,useState,type ReactNode} from "react";
import {ChevronDown} from "lucide-react";
import {MotionPresence} from "./MotionPresence";

export type MenuAction={label:string;icon?:ReactNode;disabled?:boolean;run:()=>void};
export function ActionMenu({label,icon,actions}:{label:string;icon?:ReactNode;actions:MenuAction[]}){
  const [open,setOpen]=useState(false);
  const root=useRef<HTMLDivElement>(null);
  const trigger=useRef<HTMLButtonElement>(null);
  const id=useId();
  useEffect(()=>{
    if(!open)return;
    const outside=(event:PointerEvent)=>{if(!root.current?.contains(event.target as Node))setOpen(false);};
    const escape=(event:KeyboardEvent)=>{if(event.key==="Escape"){event.preventDefault();event.stopPropagation();setOpen(false);trigger.current?.focus();}};
    document.addEventListener("pointerdown",outside);
    document.addEventListener("keydown",escape,true);
    return()=>{document.removeEventListener("pointerdown",outside);document.removeEventListener("keydown",escape,true);};
  },[open]);
  return <div className="action-menu" ref={root} onBlur={event=>{if(!event.currentTarget.contains(event.relatedTarget))setOpen(false);}}>
    <button ref={trigger} className="button secondary" type="button" aria-expanded={open} aria-controls={id} onClick={()=>setOpen(value=>!value)}>{icon}{label}<ChevronDown size={14}/></button>
    <MotionPresence>{open&&<div className="action-menu-panel glass-panel" id={id} role="group" aria-label={label}>
      {actions.map(action=><button type="button" key={action.label} disabled={action.disabled} onClick={()=>{setOpen(false);trigger.current?.focus();action.run();}}>{action.icon}<span>{action.label}</span></button>)}
    </div>}</MotionPresence>
  </div>;
}
