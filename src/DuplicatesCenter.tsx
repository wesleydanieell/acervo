import {useEffect,useRef,useState} from "react";
import {invoke} from "@tauri-apps/api/core";
import {Copy,Files,RefreshCw,X} from "lucide-react";
type DuplicateFile={path:string;format:string;associations:number};
type Group={bytes:number;sha256:string;files:DuplicateFile[]};
type Report={groups:Group[];files:number;candidates:number;hashed:number;skipped:number;failed:number;warnings:string[]};
const bytesLabel=(bytes:number)=>bytes>=1024**2?(bytes/1024**2).toFixed(1)+" MiB":bytes>=1024?(bytes/1024).toFixed(1)+" KiB":bytes+" B";
export function DuplicatesCenter({root,folders,initialFolder,onClose}:{root:string;folders:string[];initialFolder:string;onClose:()=>void}){
  const dialog=useRef<HTMLDialogElement>(null);
  const lock=useRef(false);
  const active=useRef(true);
  const [folder,setFolder]=useState(folders.includes(initialFolder)?initialFolder:"");
  const [busy,setBusy]=useState(false);
  const [report,setReport]=useState<Report|null>(null);
  const [error,setError]=useState("");
  const [copied,setCopied]=useState("");
  const [limit,setLimit]=useState(40);
  useEffect(()=>{active.current=true;dialog.current?.showModal();return()=>{active.current=false;};},[]);
  const run=async()=>{
    if(lock.current)return;
    lock.current=true;setBusy(true);setError("");setReport(null);setCopied("");setLimit(40);
    try{
      const result=await invoke<Report>("scan_duplicate_books",{root,folder:folder||null});
      if(active.current)setReport(result);
    }catch(failure){if(active.current)setError(String(failure));}
    finally{lock.current=false;if(active.current)setBusy(false);}
  };
  const copy=async(path:string)=>{
    try{await navigator.clipboard.writeText(path);if(active.current)setCopied(path);}
    catch(failure){if(active.current)setError(String(failure));}
  };
  return <dialog className="download-picker glass-panel duplicates-center" ref={dialog} aria-labelledby="duplicates-title" onCancel={event=>{event.preventDefault();onClose();}}>
    <div className="download-picker-header"><div><h2 id="duplicates-title">Arquivos duplicados</h2><p title={root}>{root}</p></div><button className="icon-button" aria-label="Fechar" onClick={onClose}><X size={19}/></button></div>
    <div className="download-picker-body organization-body">
      <label>Escopo<select className="download-name-select" value={folder} disabled={busy} onChange={e=>{setFolder(e.target.value);setReport(null);setError("");setCopied("");}}><option value="">Biblioteca inteira</option>{folder&&!folders.includes(folder)&&<option value={folder}>{folder} (indisponível)</option>}{folders.map(path=><option key={path}>{path}</option>)}</select></label>
      <button className="button secondary full" disabled={busy||!!folder&&!folders.includes(folder)} onClick={()=>void run()}>{busy?<RefreshCw className="spinning" size={17}/>:<Files size={17}/>} {busy?"Verificando conteúdo…":"Buscar duplicatas"}</button>
      {busy&&<small role="status">Comparando arquivos de mesmo tamanho. Você pode fechar a janela; a leitura atual terminará sem alterar os arquivos.</small>}
      {error&&<p role="alert" className="download-error">{error}</p>}
      {report&&<>
        <div className="metadata-summary"><div><strong>{report.files}</strong><small>Arquivos</small></div><div><strong>{report.hashed}</strong><small>Verificados</small></div><div><strong>{report.groups.length}</strong><small>Grupos idênticos</small></div><div><strong>{report.skipped+report.failed}</strong><small>Não verificados</small></div></div>
        <small>{report.candidates} candidatos de mesmo tamanho. A busca considera apenas o escopo escolhido.</small>
        {(report.skipped>0||report.failed>0||report.warnings.length>0)&&<div className="organization-review" role="status"><strong>Busca parcial ou com avisos</strong><small>{report.skipped} ignorados · {report.failed} falhas</small>{!!report.warnings.length&&<details><summary>Ver avisos</summary>{report.warnings.map((warning,i)=><p key={i}>{warning}</p>)}</details>}</div>}
        {!report.groups.length&&<p>Nenhum grupo de arquivos idênticos encontrado{report.skipped||report.failed?" entre os arquivos verificados":""}.</p>}
        <div className="duplicate-groups">{report.groups.slice(0,limit).map(group=><article className="smart-rule" key={group.sha256}>
          <div className="smart-review-head"><strong>{group.files.length} arquivos idênticos</strong><small>{bytesLabel(group.bytes)} por arquivo</small></div>
          {group.files.map(file=><div className="duplicate-file" key={file.path}><Files size={17}/><span><strong title={file.path}>{file.path}</strong><small>{file.format} · {file.associations?file.associations+" vínculo(s) no catálogo":"Sem vínculo identificado"}</small></span><button className="icon-button" title="Copiar caminho relativo" aria-label={"Copiar "+file.path} onClick={()=>void copy(file.path)}>{copied===file.path?"✓":<Copy size={15}/>}</button></div>)}
          <details><summary>SHA-256</summary><code>{group.sha256}</code></details>
        </article>)}</div>
        {report.groups.length>limit&&<button className="button secondary full" onClick={()=>setLimit(n=>n+40)}>Mostrar mais grupos</button>}
      </>}
    </div>
    <div className="download-picker-footer"><button className="button secondary" onClick={onClose}>Fechar</button></div>
  </dialog>;
}
