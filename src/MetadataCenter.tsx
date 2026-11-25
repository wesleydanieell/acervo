import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { BookOpen, RefreshCw, X } from "lucide-react";

export type EmbeddedMetadata = {title:string;author:string;language:string;publisher:string;isbn:string;description:string};
export type CachedMetadata = {metadata:EmbeddedMetadata|null;error:string|null};
type MetadataFile = {path:string;format:string;embedded?:CachedMetadata};
type BatchReport = {extracted:number;empty:number;failed:number;skipped:number;errors:string[]};
const emptyReport=():BatchReport=>({extracted:0,empty:0,failed:0,skipped:0,errors:[]});
const MAX_RUN=100;

export function MetadataCenter({root,files,cacheWarning,onClose,onChanged}:{root:string;files:MetadataFile[];cacheWarning?:string;onClose:()=>void;onChanged:()=>Promise<void>}){
  const dialog=useRef<HTMLDialogElement>(null);
  const lock=useRef(false);
  const stop=useRef(false);
  const active=useRef(true);
  const [busy,setBusy]=useState(false);
  const [stopping,setStopping]=useState(false);
  const [mode,setMode]=useState<"pending"|"retry">("pending");
  const [report,setReport]=useState<BatchReport|null>(null);
  const [progress,setProgress]=useState({done:0,total:0});
  const [error,setError]=useState("");
  const [backup,setBackup]=useState("");
  useEffect(()=>{active.current=true;dialog.current?.showModal();return()=>{active.current=false;stop.current=true;};},[]);
  const supported=useMemo(()=>files.filter(file=>["EPUB","PDF"].includes(file.format)),[files]);
  const pending=useMemo(()=>supported.filter(file=>!file.embedded),[supported]);
  const failed=useMemo(()=>supported.filter(file=>!!file.embedded?.error),[supported]);
  const withMetadata=supported.filter(file=>!!file.embedded?.metadata).length;
  const candidates=mode==="pending"?pending:failed;
  const repair=async()=>{
    if(lock.current)return;
    lock.current=true;setBusy(true);setError("");
    try{
      const name=await invoke<string>("reset_invalid_metadata_cache",{root});
      if(active.current){setBackup(name);await onChanged();}
    }catch(failure){if(active.current)setError(String(failure));}
    finally{lock.current=false;if(active.current)setBusy(false);}
  };
  const run=async()=>{
    if(lock.current||!candidates.length)return;
    const paths=candidates.slice(0,MAX_RUN).map(file=>file.path);
    lock.current=true;stop.current=false;setBusy(true);setStopping(false);setError("");setReport(null);setProgress({done:0,total:paths.length});
    const combined=emptyReport();
    let done=0;
    try{
      for(let i=0;i<paths.length&&!stop.current&&active.current;i+=10){
        const batch=paths.slice(i,i+10);
        const result=await invoke<BatchReport>("extract_book_metadata",{root,paths:batch,retry:mode==="retry"});
        combined.extracted+=result.extracted;combined.empty+=result.empty;combined.failed+=result.failed;combined.skipped+=result.skipped;
        combined.errors.push(...result.errors);
        done+=batch.length;
        if(active.current){setReport({...combined,errors:[...combined.errors]});setProgress({done,total:paths.length});}
      }
    }catch(failure){if(active.current)setError(String(failure));}
    finally{
      try{if(active.current)await onChanged();}catch(failure){if(active.current)setError(String(failure));}
      lock.current=false;
      if(active.current){setBusy(false);setStopping(false);}
    }
  };
  return <dialog className="download-picker glass-panel" ref={dialog} aria-labelledby="metadata-title" onCancel={event=>{event.preventDefault();if(!lock.current)onClose();}}>
    <div className="download-picker-header"><div><h2 id="metadata-title">Metadados dos livros</h2><p title={root}>{root}</p></div><button className="icon-button" disabled={busy} aria-label="Fechar" onClick={onClose}><X size={19}/></button></div>
    <div className="download-picker-body metadata-center">
      <div className="metadata-summary">
        <div><strong>{supported.length}</strong><small>PDF e EPUB</small></div>
        <div><strong>{withMetadata}</strong><small>Com metadados</small></div>
        <div><strong>{pending.length}</strong><small>Pendentes</small></div>
        <div><strong>{failed.length}</strong><small>Falhas</small></div>
      </div>
      {cacheWarning&&<div className="organization-review"><p role="alert">{cacheWarning}</p><button className="button secondary" disabled={busy} onClick={()=>void repair()}>Preservar cache inválido e reconstruir</button></div>}
      {backup&&<small>Cache anterior preservado: {backup}</small>}
      <label className="metadata-mode">Processar<select className="download-name-select" value={mode} disabled={busy} onChange={e=>{setMode(e.target.value==="retry"?"retry":"pending");setReport(null);setError("");setProgress({done:0,total:0});}}><option value="pending">Arquivos ainda não processados</option><option value="retry">Tentar novamente os arquivos com falha</option></select></label>
      <small>Até {MAX_RUN} arquivos por operação. Os livros originais e o catálogo não serão alterados.</small>
      {progress.total>0&&<div className="metadata-progress"><progress max={progress.total} value={progress.done}/><small>{progress.done} de {progress.total} processados{stopping?" · parando após o lote atual":""}</small></div>}
      {report&&<div className="organization-review" role="status"><strong>{report.extracted} extraídos · {report.empty} sem dados · {report.failed} falhas · {report.skipped} em cache</strong>{!!report.errors.length&&<details><summary>Detalhes das falhas</summary>{report.errors.map((message,i)=><p className="download-error" key={i}>{message}</p>)}</details>}</div>}
      {error&&<p className="download-error" role="alert">{error}</p>}
    </div>
    <div className="download-picker-footer"><button className="button secondary" disabled={stopping} onClick={()=>{if(busy){stop.current=true;setStopping(true);}else onClose();}}>{busy?"Parar após lote":"Fechar"}</button><button className="button primary" disabled={busy||!!cacheWarning||!candidates.length} onClick={()=>void run()}>{busy?<RefreshCw size={16} className="spinning"/>:<BookOpen size={16}/>} {busy?"Extraindo…":"Extrair "+Math.min(candidates.length,MAX_RUN)+" arquivo(s)"}</button></div>
  </dialog>;
}
