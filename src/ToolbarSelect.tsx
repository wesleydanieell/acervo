import {useCallback,useId,useLayoutEffect,useRef,useState,type ReactNode} from "react";
import {createPortal} from "react-dom";
import {Check,ChevronDown} from "lucide-react";
import {MotionPresence} from "./MotionPresence";
import {popupPosition} from "./popup-position";

export type ToolbarOption<T extends string>={value:T;label:string;disabled?:boolean};
export function ToolbarSelect<T extends string>({label,value,options,onChange,icon,disabled=false,className=""}:{label:string;value:T;options:readonly ToolbarOption<T>[];onChange:(value:T)=>void;icon?:ReactNode;disabled?:boolean;className?:string}){
  const id=useId();
  const trigger=useRef<HTMLButtonElement>(null);
  const panel=useRef<HTMLDivElement>(null);
  const [open,setOpen]=useState(false);
  const [portalHost,setPortalHost]=useState<HTMLElement|null>(null);
  const [position,setPosition]=useState({left:0,top:0,width:200,maxHeight:320,above:false});
  const selected=options.find(option=>option.value===value);
  const close=useCallback((restore=false)=>{
    setOpen(false);
    if(restore)trigger.current?.focus({preventScroll:true});
  },[]);
  const place=useCallback(()=>{
    const rect=trigger.current?.getBoundingClientRect();
    if(!rect)return;
    const next=popupPosition(rect,Math.max(200,rect.width),Math.min(320,options.length*38+14),window.innerWidth,window.innerHeight);
    const host=trigger.current?.closest("dialog[open]") as HTMLDialogElement|null;
    if(host){
      // A dialog's backdrop-filter (Glass) creates its own fixed containing
      // block. Always use its local absolute coordinates, across all modes.
      const bounds=host.getBoundingClientRect();
      const sx=bounds.width/host.offsetWidth||1,sy=bounds.height/host.offsetHeight||1;
      next.left=(next.left-bounds.left)/sx-host.clientLeft+host.scrollLeft;
      next.top=(next.top-bounds.top)/sy-host.clientTop+host.scrollTop;
      next.width/=sx;next.maxHeight/=sy;
    }
    setPosition(next);
  },[options.length]);
  const show=()=>{
    const host=trigger.current?.closest("dialog[open]") as HTMLElement|null;
    setPortalHost(host??document.body);
    place();setOpen(true);
  };
  useLayoutEffect(()=>{
    if(!open)return;
    place();
    const focus=()=>{const item=panel.current?.querySelector<HTMLButtonElement>('[aria-checked="true"]:not(:disabled)')??panel.current?.querySelector<HTMLButtonElement>("button:not(:disabled)");if(!item)return;
      item.focus({preventScroll:true});
      const container=panel.current;
      if(container){
        // Layout coordinates are not scaled by the opening animation.
        const top=item.offsetTop,bottom=top+item.offsetHeight;
        if(bottom>container.scrollTop+container.clientHeight)container.scrollTop=bottom-container.clientHeight+5;
        else if(top<container.scrollTop)container.scrollTop=Math.max(0,top-5);
      }};
    focus();
    const outside=(event:PointerEvent)=>{
      if(!trigger.current?.contains(event.target as Node)&&!panel.current?.contains(event.target as Node))close();
    };
    const scroll=(event:Event)=>{if(!panel.current?.contains(event.target as Node))close();};
    const resize=()=>close(true);
    // Reposition once the containing dialog's entrance has settled.
    const settled=window.setTimeout(place,360);
    window.addEventListener("pointerdown",outside,true);
    window.addEventListener("scroll",scroll,true);
    window.addEventListener("resize",resize);
    return()=>{window.clearTimeout(settled);window.removeEventListener("pointerdown",outside,true);window.removeEventListener("scroll",scroll,true);window.removeEventListener("resize",resize);};
  },[open,place,close]);
  useLayoutEffect(()=>{if(disabled)close();},[disabled,close]);
  return <>
    <button type="button" ref={trigger} className={"pill-control toolbar-select "+className} aria-label={label+": "+(selected?.label??value)} aria-haspopup="menu" aria-expanded={open} aria-controls={open?id:undefined} disabled={disabled}
      onClick={()=>{if(open)close();else show()}}
      onKeyDown={event=>{if(["ArrowDown","ArrowUp"].includes(event.key)){event.preventDefault();if(!open)show()}}}>
      {icon}<span className="toolbar-select-label" title={selected?.label}>{selected?.label??value}</span><ChevronDown className="toolbar-select-chevron" size={14} aria-hidden="true"/>
    </button>
    {portalHost&&createPortal(<MotionPresence>{open&&<div ref={panel} id={id} className="acervo-popup toolbar-select-popup" role="menu" aria-label={label} data-placement={position.above?"above":"below"}
      style={{left:position.left,top:position.top,width:position.width,maxHeight:position.maxHeight}}
      onContextMenu={event=>{event.preventDefault();event.stopPropagation();}}
      onBlur={event=>{if(event.relatedTarget&&!event.currentTarget.contains(event.relatedTarget)&&event.relatedTarget!==trigger.current)close();}}
      onKeyDown={event=>{
        const keys=["ArrowUp","ArrowDown","Home","End","Escape","Tab"];
        if(!keys.includes(event.key))return;
        if(event.key==="Escape"){event.preventDefault();event.stopPropagation();close(true);return;}
        if(event.key==="Tab"){close(true);return;}
        const buttons=[...event.currentTarget.querySelectorAll<HTMLButtonElement>("button:not(:disabled)")];
        if(!buttons.length)return;
        event.preventDefault();
        const active=buttons.indexOf(document.activeElement as HTMLButtonElement);
        const next=event.key==="Home"?0:event.key==="End"?buttons.length-1:(active+(event.key==="ArrowDown"?1:-1)+buttons.length)%buttons.length;
        buttons[next]?.focus();
      }}>
      {options.map(option=><button type="button" key={option.value} role="menuitemradio" aria-checked={option.value===value} disabled={option.disabled}
        onClick={()=>{close(true);onChange(option.value);}}><span>{option.label}</span>{option.value===value&&<Check size={16} aria-hidden="true"/>}</button>)}
    </div>}</MotionPresence>,portalHost)}
  </>;
}
