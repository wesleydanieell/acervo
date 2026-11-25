import {useEffect,useMemo,useRef,useState} from "react";
import {invoke} from "@tauri-apps/api/core";
import {BookOpen,Check,ChevronDown,ChevronRight,ClipboardList,Copy,Files,RefreshCw,Search,Shield,X} from "lucide-react";
import {fileName,folderName,filterGroups,protections,reviewGroup,exportReview,type DuplicateGroup,type ReviewFilter,type ReviewSort} from "./duplicate-review";
type Report={groups:DuplicateGroup[];files:number;candidates:number;hashed:number;skipped:number;failed:number;warnings:string[]};
const bytesLabel=(bytes:number)=>bytes>=1024**3?(bytes/1024**3).toFixed(2)+" GiB":bytes>=1024**2?(bytes/1024**2).toFixed(1)+" MiB":bytes>=1024?(bytes/1024).toFixed(1)+" KiB":bytes+" B";
export function DuplicatesCenter({root,folders,initialFolder,onClose,onInspect}:{root:string;folders:string[];initialFolder:string;onClose:()=>void;onInspect:(path:string)=>void}){
  const dialog=useRef<HTMLDialogElement>(null);
  const lock=useRef(false);
  const active=useRef(true);
  const [folder,setFolder]=useState(folders.includes(initialFolder)?initialFolder:"");
  const [busy,setBusy]=useState(false);
  const [report,setReport]=useState<Report|null>(null);
  const [error,setError]=useState("");
  const [copied,setCopied]=useState("");
  const [limit,setLimit]=useState(40);
  const [query,setQuery]=useState("");
  const [filter,setFilter]=useState<ReviewFilter>("all");
  const [sort,setSort]=useState<ReviewSort>("volume");
  const [choices,setChoices]=useState<Record<string,string>>({});
  const [expanded,setExpanded]=useState<Set<string>>(new Set());
  const [reviewOnly,setReviewOnly]=useState(false);
  const [fileLimits,setFileLimits]=useState<Record<string,number>>({});
  const groups=useMemo(()=>filterGroups(report?.groups??[],query,filter,sort,choices),[report,query,filter,sort,choices]);
  const review=useMemo(()=>groups.map(group=>reviewGroup(group,choices[group.sha256])),[groups,choices]);
  const count=review.reduce((n,plan)=>n+plan.reviewable.length,0);
  const volume=review.reduce((n,plan)=>n+plan.logicalBytes,0);
  useEffect(()=>{active.current=true;dialog.current?.showModal();return()=>{active.current=false;};},[]);
  const run=async()=>{
    if(lock.current)return;
    lock.current=true;setBusy(true);setError("");setReport(null);setCopied("");setLimit(40);setChoices({});setExpanded(new Set());setReviewOnly(false);setFileLimits({});
    try{
      const result=await invoke<Report>("scan_duplicate_books",{root,folder:folder||null});
      if(active.current){setReport(result);setExpanded(new Set(result.groups.slice(0,1).map(group=>group.sha256)));}
    }catch(failure){if(active.current)setError(String(failure));}
    finally{lock.current=false;if(active.current)setBusy(false);}
  };
  const copy=async(text:string,id:string)=>{
    try{await navigator.clipboard.writeText(text);if(active.current)setCopied(id);}
    catch(failure){if(active.current)setError(String(failure));}
  };
  return <dialog className="download-picker glass-panel duplicates-center" ref={dialog} aria-labelledby="duplicates-title" onCancel={event=>{event.preventDefault();onClose();}}>
    <div className="download-picker-header"><div><h2 id="duplicates-title">Arquivos duplicados</h2><p title={root}>{root}</p></div><button className="icon-button" aria-label="Fechar" onClick={onClose}><X size={19}/></button></div>
    <div className="download-picker-body organization-body">
      <div className="duplicate-scan-bar"><label>Escopo<select className="download-name-select" value={folder} disabled={busy} onChange={e=>{setFolder(e.target.value);setReport(null);setError("");setCopied("");}}><option value="">Biblioteca inteira</option>{folder&&!folders.includes(folder)&&<option value={folder}>{folder} (indisponível)</option>}{folders.map(path=><option key={path}>{path}</option>)}</select></label>
      <button className="button primary" disabled={busy||!!folder&&!folders.includes(folder)} onClick={()=>void run()}>{busy?<RefreshCw className="spinning" size={17}/>:<Search size={17}/>} {busy?"Verificando…":report?"Verificar novamente":"Buscar duplicatas"}</button></div>
      {busy&&<small role="status">Comparando conteúdo de arquivos de mesmo tamanho…</small>}
      {error&&<p role="alert" className="download-error">{error}</p>}
      {report&&<>
        <div className="metadata-summary"><div><strong>{report.groups.length}</strong><small>Grupos idênticos</small></div><div><strong>{report.groups.reduce((n,g)=>n+g.files.length-1,0)}</strong><small>Cópias adicionais</small></div><div><strong>{bytesLabel(report.groups.reduce((n,g)=>n+g.bytes*(g.files.length-1),0))}</strong><small>Volume lógico</small></div><div><strong>{report.hashed}/{report.candidates}</strong><small>Candidatos verificados</small></div></div>
        {(report.skipped>0||report.failed>0||report.warnings.length>0)&&<div className="organization-review" role="status"><strong>Busca parcial ou com avisos</strong><small>{report.skipped} ignorados · {report.failed} falhas</small>{!!report.warnings.length&&<details><summary>Ver avisos</summary>{report.warnings.map((warning,i)=><p key={i}>{warning}</p>)}</details>}</div>}
        {!report.groups.length?<p>Nenhum grupo de arquivos idênticos encontrado entre os arquivos verificados.</p>:<>
          <div className="duplicate-filter-bar"><label className="duplicate-search"><Search size={16}/><input aria-label="Pesquisar duplicatas" placeholder="Nome, pasta ou hash" value={query} onChange={e=>{setQuery(e.target.value);setLimit(40);}}/></label><select aria-label="Filtrar grupos" value={filter} onChange={e=>{setFilter(e.target.value as ReviewFilter);setLimit(40);}}><option value="all">Todos os grupos</option><option value="reviewable">Com cópias para revisar</option><option value="protected">Com registros protegidos</option></select><select aria-label="Ordenar grupos" value={sort} onChange={e=>{setSort(e.target.value as ReviewSort);setLimit(40);}}><option value="volume">Maior volume</option><option value="copies">Mais cópias</option><option value="name">Nome</option></select></div>
          <div className="duplicate-plan-bar"><span><strong>{groups.length} {groups.length===1?"grupo":"grupos"}</strong> · {count} {count===1?"cópia":"cópias"} para revisar · {bytesLabel(volume)}</span><button className="button secondary" onClick={()=>setReviewOnly(v=>!v)}>{reviewOnly?"Ver todos os arquivos":"Prévia de resolução"}</button></div>
          <small>Selecione a cópia a manter. Registros vinculados são protegidos. Confira novamente após alterações nos arquivos.</small>
          {!groups.length&&<p>Nenhum grupo corresponde aos filtros.</p>}
          <div className="duplicate-groups">{groups.slice(0,limit).map(group=>{
            const plan=reviewGroup(group,choices[group.sha256]);
            const open=expanded.has(group.sha256)||reviewOnly;
            const fileLimit=fileLimits[group.sha256]??60;
            const ordered=[...group.files].sort((a,b)=>Number(b.path===plan.keeper)-Number(a.path===plan.keeper));
            return <article className="smart-rule duplicate-group" key={group.sha256}>
              <button className="duplicate-group-header" aria-expanded={open} onClick={()=>{if(reviewOnly)setReviewOnly(false);setExpanded(old=>{const next=new Set(old);if(next.has(group.sha256))next.delete(group.sha256);else next.add(group.sha256);return next;});}}>
                {open?<ChevronDown size={16}/>:<ChevronRight size={16}/>}<Files size={19}/><span><strong title={plan.keeper}>{fileName(plan.keeper)}</strong><small>{group.files.length} arquivos · {bytesLabel(group.bytes)} cada</small></span><small>{plan.blocked.length>0&&<Shield size={15}/>} {plan.reviewable.length} para revisar</small>
              </button>
              {open&&<div className="duplicate-group-body">{ordered.slice(0,fileLimit).map(file=>{
                const reasons=protections(file);
                const keep=file.path===plan.keeper;
                return <div className={"duplicate-file"+(keep?" duplicate-keeper":"")} key={file.path}>
                  <div className="duplicate-file-choice"><input type="radio" name={"keep-"+group.sha256} checked={keep} aria-label={"Manter "+file.path} onChange={()=>setChoices(old=>({...old,[group.sha256]:file.path}))}/><div className="duplicate-file-text"><button className="duplicate-file-name" type="button" title={file.path} onClick={()=>setChoices(old=>({...old,[group.sha256]:file.path}))}>{fileName(file.path)}</button><small className="duplicate-location" title={folderName(file.path)}>{folderName(file.path)} <span aria-hidden="true">·</span> {file.format}</small>{reasons.length>0&&<small className="duplicate-protection"><Shield size={12} aria-hidden="true"/><span>{reasons.join(" · ")}</span></small>}</div></div><span className={"duplicate-row-status"+(keep?" duplicate-keep-label":"")}>{keep?"Manter":reasons.length?"Protegido":reviewOnly?"Revisar":"Cópia"}</span>
                  <button className="icon-button" title="Ver detalhes do livro" aria-label={"Ver detalhes de "+file.path} onClick={()=>onInspect(file.path)}><BookOpen size={16}/></button>
                  <button className="icon-button" title="Copiar caminho relativo" aria-label={"Copiar "+file.path} onClick={()=>void copy(file.path,file.path)}>{copied===file.path?<Check size={15}/>:<Copy size={15}/>}</button>
                </div>;
              })}{ordered.length>fileLimit&&<button className="button secondary full" onClick={()=>setFileLimits(old=>({...old,[group.sha256]:fileLimit+60}))}>Mostrar mais arquivos ({fileLimit} de {ordered.length})</button>}<details className="duplicate-hash"><summary>SHA-256</summary><code>{group.sha256}</code></details></div>}
            </article>;
          })}</div>
          {groups.length>limit&&<button className="button secondary full" onClick={()=>setLimit(n=>n+40)}>Mostrar mais grupos ({Math.min(limit,groups.length)} de {groups.length})</button>}
        </>}
      </>}
    </div>
    <div className="download-picker-footer"><small className="duplicate-footer-note">Somente revisão · sem exclusão</small><button className="button secondary" disabled={!groups.length||busy} onClick={()=>void copy(exportReview(groups,choices),"review")}>{copied==="review"?<Check size={16}/>:<ClipboardList size={16}/>} {copied==="review"?"Prévia copiada":"Copiar prévia"}</button><button className="button secondary" onClick={onClose}>Fechar</button></div>
  </dialog>;
}
