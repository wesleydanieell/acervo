import { useEffect,useRef,useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Download,RefreshCw,X } from "lucide-react";
import type { SourceResult } from "./discovery-pagination";

type AcquisitionLink={url:string;format:string};
export function AcquisitionPicker({book,onClose,onChoose}:{book:SourceResult;onClose:()=>void;onChoose:(book:SourceResult)=>void}){
  const dialog=useRef<HTMLDialogElement>(null);
  const [links,setLinks]=useState<AcquisitionLink[]|null>(null);
  const [error,setError]=useState("");
  const [loading,setLoading]=useState(true);
  const [attempt,setAttempt]=useState(0);
  useEffect(()=>{dialog.current?.showModal();},[]);
  useEffect(()=>{
    let alive=true;
    const timer=window.setTimeout(()=>{
      setLoading(true);setError("");setLinks(null);
      void invoke<AcquisitionLink[]>("resolve_source_downloads",{id:book.sourceId,page:book.acquisitionPage}).then(result=>{
        if(!alive)return;
        const known=["pdf","epub","mobi","azw3","djvu"].includes(book.format.toLowerCase());
        setLinks(known?result.filter(item=>item.format===book.format.toLowerCase()):result);
      }).catch(failure=>{if(alive)setError(String(failure));}).finally(()=>{if(alive)setLoading(false);});
    },0);
    return()=>{alive=false;window.clearTimeout(timer);};
  },[book.sourceId,book.acquisitionPage,book.format,attempt]);
  return <dialog ref={dialog} className="download-picker glass-panel" aria-labelledby="acquisition-picker-title" onCancel={event=>{event.preventDefault();onClose();}}>
    <div className="download-picker-header"><div><h2 id="acquisition-picker-title">Arquivos disponíveis</h2><p>{book.title} · {book.sourceName}</p></div><button className="icon-button" aria-label="Fechar" onClick={onClose}><X size={19}/></button></div>
    <div className="download-picker-body">
      {loading&&<p className="feed-empty" role="status"><RefreshCw className="spinning" size={16}/> Consultando página de aquisição…</p>}
      {error&&<p className="download-error" role="alert">{error}</p>}
      {links?.length===0&&<p className="feed-empty">Nenhum arquivo compatível encontrado.</p>}
      {!!links?.length&&<div className="download-picker-list">{links.map(item=><button key={item.url} className="download-picker-row" onClick={()=>onChoose({...book,format:item.format,downloadUrl:item.url,acquisitionPage:undefined})}><Download size={17}/><span><strong>{item.format.toUpperCase()}</strong><small title={item.url}>{new URL(item.url).hostname}{new URL(item.url).pathname}</small></span></button>)}</div>}
    </div>
    <div className="download-picker-footer"><button className="button secondary" onClick={onClose}>Cancelar</button>{!loading&&<button className="button secondary" onClick={()=>setAttempt(n=>n+1)}><RefreshCw size={16}/> Atualizar</button>}</div>
  </dialog>;
}
