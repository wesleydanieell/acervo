import {useCallback,useLayoutEffect,useMemo,useRef,useState,type RefObject,type ReactNode} from "react";
import {listWindow,navigationIndex,scrollTarget} from "./virtual-list";
const PITCH=104;

export function VirtualBookList<T extends {key:string}>({items,scrollRef,render}:{items:readonly T[];scrollRef:RefObject<HTMLElement|null>;render:(item:T,tabIndex:number)=>ReactNode}){
  const list=useRef<HTMLDivElement>(null);
  const [window,setWindow]=useState({start:0,end:Math.min(items.length,16)});
  const [activeKey,setActiveKey]=useState(items[0]?.key??"");
  const [focused,setFocused]=useState(false);
  const [dragged,setDragged]=useState<string|null>(null);
  const focusRequest=useRef<string|null>(null);
  const indexByKey=useMemo(()=>new Map(items.map((item,index)=>[item.key,index])),[items]);
  const active=indexByKey.get(activeKey)??Math.min(window.start,Math.max(0,items.length-1));
  const metrics=useCallback(()=>{
    const parent=scrollRef.current,element=list.current;
    if(!parent||!element)return null;
    const offset=parent.getBoundingClientRect().top-element.getBoundingClientRect().top;
    return {parent,offset,viewport:parent.clientHeight};
  },[scrollRef]);
  const measure=useCallback(()=>{
    const m=metrics();if(!m)return;
    const next=listWindow(items.length,m.offset,m.viewport,PITCH);
    setWindow(old=>old.start===next.start&&old.end===next.end?old:next);
    if(!list.current?.contains(document.activeElement)&&!focusRequest.current&&next.end>next.start){
      const index=Math.max(next.start,Math.min(next.end-1,Math.floor(Math.max(0,m.offset)/PITCH)));
      const key=items[index]?.key;
      if(key)setActiveKey(old=>{const position=indexByKey.get(old);return position!==undefined&&position>=next.start&&position<next.end?old:key;});
    }
  },[items,indexByKey,metrics]);
  useLayoutEffect(()=>{
    const parent=scrollRef.current,element=list.current;if(!parent||!element)return;
    let frame=0;
    const schedule=()=>{if(!frame)frame=requestAnimationFrame(()=>{frame=0;measure();});};
    const observer=new ResizeObserver(schedule);
    observer.observe(parent);observer.observe(element);
    for(const sibling of Array.from(parent.children))if(sibling!==element)observer.observe(sibling);
    parent.addEventListener("scroll",schedule,{passive:true});measure();
    return()=>{observer.disconnect();parent.removeEventListener("scroll",schedule);if(frame)cancelAnimationFrame(frame);};
  },[measure,scrollRef]);
  useLayoutEffect(()=>{measure();});
  useLayoutEffect(()=>{
    const key=focusRequest.current;
    if(!key)return;
    const index=indexByKey.get(key);
    const row=index===undefined?null:list.current?.querySelector<HTMLElement>('[data-index="'+index+'"] > button');
    if(row){focusRequest.current=null;row.focus({preventScroll:true});}
  });
  const rendered=new Set<number>();
  for(let i=window.start;i<window.end;i++)if(i<items.length)rendered.add(i);
  if(items.length&&(focused||focusRequest.current||rendered.size===0))rendered.add(active);
  const dragIndex=dragged===null?undefined:indexByKey.get(dragged);
  if(dragIndex!==undefined)rendered.add(dragIndex);
  const tabIndex=rendered.has(active)?active:Math.min(...rendered);
  return <div className="books-list virtual-book-list" ref={list} style={{height:items.length*PITCH}} role="list" aria-label={"Livros ("+items.length+"). Use as setas, Home, End, Page Up e Page Down para navegar."}
    onBlur={event=>{if(!event.currentTarget.contains(event.relatedTarget)){setFocused(false);measure();}}}
    onKeyDown={event=>{
      if(event.altKey||event.ctrlKey||event.metaKey||event.shiftKey)return;
      const m=metrics();if(!m)return;
      const next=navigationIndex(event.key,active,items.length,Math.max(1,Math.floor(m.viewport/PITCH)));
      if(next===null)return;
      event.preventDefault();
      const key=items[next].key;
      focusRequest.current=key;setActiveKey(key);setFocused(true);
      const offset=scrollTarget(next,PITCH,m.offset,m.viewport);
      m.parent.scrollTop+=offset-m.offset;
      measure();
      if(next===active){focusRequest.current=null;}
    }}>
    {[...rendered].sort((a,b)=>a-b).map(index=>{
      const item=items[index];if(!item)return null;
      return <div className="virtual-book-row" key={item.key} data-index={index} role="listitem" aria-posinset={index+1} aria-setsize={items.length} style={{top:index*PITCH,height:PITCH-6}}
        onFocus={()=>{setActiveKey(item.key);setFocused(true);}}
        onDragStart={()=>setDragged(item.key)} onDragEnd={()=>setDragged(null)}>
        {render(item,index===tabIndex?0:-1)}
      </div>;
    })}
  </div>;
}
