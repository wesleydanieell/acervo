import { memo,useEffect,useRef,useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { BookOpen } from "lucide-react";

const cache=new Map<string,string>();
const pending=new Map<string,Promise<string|null>>();
const queue:(()=>void)[]=[];
let running=0;
function drain(){
  while(running<4&&queue.length){running++;queue.shift()!();}
}
function fetchCover(id:string,url:string){
  const key=JSON.stringify([id,url]);
  const existing=cache.get(key);
  if(existing)return Promise.resolve(existing);
  const active=pending.get(key);if(active)return active;
  if(queue.length>=96)return Promise.resolve(null);
  const request=new Promise<string|null>(resolve=>{
    queue.push(()=>{
      void invoke<string|null>("get_source_cover",{id,url}).catch(()=>null).then(value=>{
        if(value){cache.set(key,value);if(cache.size>48)cache.delete(cache.keys().next().value!);}
        resolve(value);
      }).finally(()=>{pending.delete(key);running--;drain();});
    });
  });
  pending.set(key,request);drain();return request;
}
export const SourceCover=memo(function SourceCover({sourceId,url,title}:{sourceId:string;url?:string;title:string}){
  const target=useRef<HTMLDivElement>(null);
  const key=JSON.stringify([sourceId,url]);
  const [loaded,setLoaded]=useState<{key:string;value:string|null}|null>(null);
  const value=loaded?.key===key?loaded.value:null;
  useEffect(()=>{
    if(!url||!target.current)return;
    let alive=true;
    const load=()=>{void fetchCover(sourceId,url).then(value=>{if(alive)setLoaded({key,value});});};
    const observer=typeof IntersectionObserver!=="undefined"?new IntersectionObserver(([entry])=>{if(entry.isIntersecting){observer?.disconnect();load();}},{rootMargin:"100px"}):null;
    if(observer)observer.observe(target.current);else load();
    return()=>{alive=false;observer?.disconnect();};
  },[sourceId,url,key]);
  return <div ref={target} className="feed-book-icon source-cover">{value?<img src={value} alt={"Capa de "+title} draggable={false} onError={()=>setLoaded({key,value:null})}/>:<BookOpen size={23}/>}</div>;
});
