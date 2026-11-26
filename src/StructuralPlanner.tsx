import {useEffect,useMemo,useRef,useState,type ReactNode} from "react";
import {invoke} from "@tauri-apps/api/core";
import {ArrowRight,Folder,FolderPlus,FolderTree,RefreshCw,Search,Trash2,X} from "lucide-react";
import {draftOperation,draftParents,impactCounts,projectFolders,structuralPath,type StructuralOperation,type StructuralPreview,type PathChange} from "./structural-plan";
import {parseRules,ruleStorageKey,type OrganizationRules} from "./organization-rules";
import "./structural-planner.css";

const reasons:Record<string,string>={
  missingOrInvalidBook:"Arquivo ausente ou inválido",staleManualIdentity:"Identidade de metadados divergente",staleOrUnreadableEdition:"Edição divergente ou ilegível",editionVerificationDeferred:"Verificação de edição pendente",pendingImport:"Importação pendente",unknownLibrary:"Biblioteca do download não identificada",invalidDestination:"Destino inválido",staleCatalogTarget:"Ficha de aquisição desatualizada",activeTransfer:"Transferência ativa"
};
function PathRow({change}:{change:PathChange}){return <div className="organization-move"><span>{change.before||"Raiz"}</span><ArrowRight size={14}/><strong>{change.after||"Raiz"}</strong></div>}
function ImpactList<T>({title,items,render,initialOpen=false}:{title:string;items:readonly T[];render:(item:T,index:number)=>ReactNode;initialOpen?:boolean}){
  const [open,setOpen]=useState(initialOpen);
  const [limit,setLimit]=useState(80);
  return <details open={open} onToggle={e=>setOpen(e.currentTarget.open)}><summary>{title} ({items.length})</summary>{open&&<>{items.slice(0,limit).map(render)}{items.length>limit&&<button className="button secondary" onClick={()=>setLimit(n=>n+80)}>Mostrar mais ({items.length-limit})</button>}</>}</details>
}
function FolderChoice({label,paths,value,onChange,disabled,rootChoice=false}:{label:string;paths:readonly string[];value:string;onChange:(value:string)=>void;disabled:boolean;rootChoice?:boolean}){
  const [query,setQuery]=useState("");
  const [limit,setLimit]=useState(100);
  const filtered=paths.filter(path=>path.toLocaleLowerCase("pt-BR").includes(query.toLocaleLowerCase("pt-BR")));
  const options=filtered.slice(0,limit);
  if(value&&paths.includes(value)&&!options.includes(value))options.unshift(value);
  return <div className="structural-choice"><label>{label}<select disabled={disabled} value={value} onChange={e=>onChange(e.target.value)}><option value="">{rootChoice?"Raiz da biblioteca":"Escolher pasta…"}</option>{options.map(path=><option key={path}>{path}</option>)}</select></label><div className="destination-search"><Search size={14}/><input disabled={disabled} aria-label={"Filtrar "+label.toLowerCase()} value={query} onChange={e=>{setQuery(e.target.value);setLimit(100);}} placeholder="Filtrar pastas…"/></div>{filtered.length>limit&&<button className="button secondary" disabled={disabled} onClick={()=>setLimit(n=>n+100)}>Mais opções ({filtered.length-limit})</button>}</div>
}
function FolderList({title,paths}:{title:string;paths:readonly string[]}){
  const [query,setQuery]=useState("");
  const [limit,setLimit]=useState(80);
  const shown=paths.filter(path=>path.toLocaleLowerCase("pt-BR").includes(query.toLocaleLowerCase("pt-BR")));
  return <section className="structural-tree"><div className="structural-tree-heading"><strong>{title}</strong><small>{paths.length} pastas</small></div>
    <label className="destination-search"><Search size={14}/><input value={query} onChange={e=>{setQuery(e.target.value);setLimit(80);}} placeholder="Buscar pasta" aria-label={"Buscar em "+title}/></label>
    <div className="structural-tree-list">{shown.slice(0,limit).map(path=><div key={path} title={path}><Folder size={15}/><span>{path}</span></div>)}{!shown.length&&<small>Nenhuma pasta.</small>}{shown.length>limit&&<button className="button secondary" onClick={()=>setLimit(n=>n+80)}>Mostrar mais ({shown.length-limit})</button>}</div>
  </section>
}
export function StructuralPlanner({root,folders,initialFolder,onClose}:{root:string;folders:readonly string[];initialFolder:string;onClose:()=>void}){
  const dialog=useRef<HTMLDialogElement>(null);
  const active=useRef(true);
  const lock=useRef(false);
  const [kind,setKind]=useState<StructuralOperation["kind"]>("moveFolder");
  const [source,setSource]=useState(initialFolder&&structuralPath(initialFolder)?initialFolder:"");
  const [parent,setParent]=useState("");
  const [name,setName]=useState(initialFolder.split("/").pop()||"");
  const [operations,setOperations]=useState<StructuralOperation[]>([]);
  const [preview,setPreview]=useState<StructuralPreview|null>(null);
  const [error,setError]=useState("");
  const [ruleError,setRuleError]=useState("");
  const [busy,setBusy]=useState(false);
  const [verified,setVerified]=useState(false);
  useEffect(()=>{active.current=true;dialog.current?.showModal();return()=>{active.current=false;};},[]);
  const actual=useMemo(()=>projectFolders(folders,[]),[folders]);
  const projected=useMemo(()=>projectFolders(folders,operations),[folders,operations]);
  const parents=useMemo(()=>draftParents(folders,operations,kind==="moveFolder"?source:""),[folders,operations,kind,source]);
  const sources=actual.filter(path=>!operations.some(op=>op.kind==="moveFolder"&&(path===op.source||path.startsWith(op.source+"/")||op.source.startsWith(path+"/"))));
  const invalidate=()=>{setPreview(null);setVerified(false);setError("");setRuleError("");};
  const add=()=>{
    try{
      if(operations.length>=100)throw new Error("Limite de 100 ações por proposta.");
      if(kind==="moveFolder"&&!sources.includes(source))throw new Error("Selecione uma pasta de origem disponível.");
      if(parent&&!parents.includes(parent))throw new Error("Selecione um destino disponível.");
      const op=draftOperation(kind,source,parent,name.trim());
      const target=op.kind==="createFolder"?op.path:op.target;
      if(projected.includes(target))throw new Error("Já existe uma pasta nesse destino.");
      setOperations(old=>[...old,op]);invalidate();
      setName("");setSource("");setParent("");
    }catch(failure){setError(String(failure));}
  };
  const run=async(revalidate:boolean)=>{
    if(lock.current||!operations.length)return;
    lock.current=true;setBusy(true);setError("");setVerified(false);
    const previous=preview;
    setPreview(null);
    let rules:OrganizationRules|null=null;
    try{rules=parseRules(localStorage.getItem(ruleStorageKey(root)));setRuleError("");}
    catch(failure){setRuleError("Regras não analisadas: "+String(failure));}
    try{
      const result=await invoke<StructuralPreview>("preview_library_restructure",{root,operations,rules,expectedFilesystemRevision:revalidate?previous?.filesystemRevision??null:null,expectedPlanRevision:revalidate?previous?.planRevision??null:null});
      if(active.current){setPreview(result);setVerified(revalidate);}
    }catch(failure){if(active.current)setError(String(failure));}
    finally{lock.current=false;if(active.current)setBusy(false);}
  };
  const counts=preview?impactCounts(preview):null;
  return <dialog ref={dialog} className="download-picker glass-panel structural-planner" aria-labelledby="structural-title" onCancel={e=>{e.preventDefault();onClose();}}>
    <header className="download-picker-header"><div><h2 id="structural-title">Planejar estrutura</h2><p title={root}>{root}</p></div><button className="icon-button" onClick={onClose} aria-label="Fechar"><X size={19}/></button></header>
    <div className="download-picker-body structural-body">
      <div className="structural-comparison"><FolderList title="Estrutura atual" paths={actual}/><FolderList title="Proposta local" paths={projected}/></div>
      <section className="structural-editor">
        <div className="smart-review-head"><strong>Adicionar ação</strong><small>{operations.length}/100</small></div>
        <div className="structural-fields">
          <label>Ação<select disabled={busy} value={kind} onChange={e=>{setKind(e.target.value as StructuralOperation["kind"]);setSource("");setParent("");setName("");}}><option value="moveFolder">Mover / renomear pasta</option><option value="createFolder">Criar pasta</option></select></label>
          {kind==="moveFolder"&&<FolderChoice label="Origem" paths={sources} value={source} disabled={busy} onChange={path=>{setSource(path);setName(path.split("/").pop()||"");setParent("");}}/>}
          <FolderChoice label="Pasta pai" paths={parents} value={parent} disabled={busy} rootChoice onChange={setParent}/>
          <label>Nome final<input disabled={busy} maxLength={255} value={name} onChange={e=>setName(e.target.value)} placeholder="Nome da pasta"/></label>
        </div>
        <button className="button secondary" disabled={busy||operations.length>=100||!name.trim()||kind==="moveFolder"&&!source} onClick={add}><FolderPlus size={16}/> Adicionar à proposta</button>
      </section>
      <section className="structural-editor">
        <div className="smart-review-head"><strong>Ações propostas</strong><button className="button secondary" disabled={busy||!operations.length} onClick={()=>{setOperations([]);invalidate();setParent("");}}>Limpar</button></div>
        {!operations.length&&<small>Adicione uma ação para comparar as estruturas.</small>}
        {operations.map((op,i)=><div className="structural-action" key={i}><FolderTree size={17}/><span><small>{op.kind==="createFolder"?"Criar pasta":"Mover / renomear"}</small><strong>{op.kind==="createFolder"?op.path:op.source+" → "+op.target}</strong></span><button className="icon-button" disabled={busy} aria-label={"Remover ação "+(i+1)} onClick={()=>{setOperations(old=>old.filter((_,index)=>index!==i));invalidate();setParent("");}}><Trash2 size={16}/></button></div>)}
      </section>
      {error&&<div className="warning" role="alert">{error}</div>}
      {ruleError&&<div className="warning" role="alert">{ruleError}</div>}
      {preview&&counts&&<section className="organization-review structural-impact" aria-live="polite">
        <div className="smart-review-head"><strong>{verified?"Prévia revalidada":"Prévia do backend"}</strong><span>Somente leitura</span></div>
        <div className="metadata-summary">{[[preview.createdFolders.length,"Pastas novas"],[counts.books,"Livros afetados"],[counts.works,"Obras afetadas"],[counts.blockers,"Bloqueios"]].map(([n,label])=><div key={label}><strong>{n}</strong><small>{label}</small></div>)}</div>
        {preview.movedFolders.map(folder=><div key={folder.source}><PathRow change={{before:folder.source,after:folder.target}}/><small>{folder.folders} pastas · {folder.files} arquivos · {folder.books} livros</small></div>)}
        {preview.referenceImpact.works.length>0&&<ImpactList title="Obras e destinos planejados" items={preview.referenceImpact.works} render={work=><article key={work.workId}><strong>{work.title}</strong>{work.plannedDestination&&<><small>Destino planejado</small><PathRow change={work.plannedDestination}/></>}{work.associatedFiles.map((change,i)=><PathRow key={i} change={change}/>)}</article>}/>}
        {([["Metadados manuais",preview.referenceImpact.manualMetadata],["Edições",preview.referenceImpact.editions]] as const).map(([label,changes])=>changes.length>0&&<ImpactList key={label} title={label} items={changes} render={change=><PathRow key={change.before} change={change}/>}/>)}
        <small>Edições: {preview.referenceImpact.editionVerification.checked} verificações tentadas · {preview.referenceImpact.editionVerification.deferred} adiadas</small>
        {preview.downloadImpact===null?<small>Downloads não analisados.</small>:<ImpactList title="Downloads afetados" items={preview.downloadImpact.changes} render={item=><article key={item.id}><strong>{item.title}</strong>{item.plannedDestination&&<PathRow change={item.plannedDestination}/>} {item.catalogDestination&&<PathRow change={item.catalogDestination}/>}</article>}/>}
        {preview.ruleImpact===null?<small>Regras não analisadas.</small>:<ImpactList title="Regras afetadas" items={preview.ruleImpact} render={(item,i)=><article key={i}><strong>{item.ruleName} {item.enabled?"":"(desativada)"}</strong><PathRow change={item.destination}/></article>}/>}
        {counts.blockers>0&&<ImpactList title="Bloqueios" initialOpen items={[...preview.referenceImpact.blockers.map(item=>(reasons[item.reason]??item.reason)+": "+item.path),...(preview.downloadImpact?.blockers??[]).map(item=>(reasons[item.reason]??item.reason)+": "+item.title),...(preview.downloadImpact?.destinationConflicts??[]).map(item=>"Conflito de downloads: "+item.target)]} render={(message,i)=><p key={i} className="download-error">{message}</p>}/>}
      </section>}
    </div>
    <footer className="download-picker-footer"><small className="structural-footer-note">Nenhum arquivo será alterado. Aplicação estrutural ainda indisponível.</small><button className="button secondary" onClick={onClose}>Fechar</button>{preview&&<button className="button secondary" disabled={busy||!preview.planRevision} onClick={()=>void run(true)}><RefreshCw size={15}/> Revalidar</button>}<button className="button primary" disabled={busy||!operations.length} onClick={()=>void run(false)}>{busy?<RefreshCw className="spinning" size={16}/>:<Search size={16}/>} Gerar prévia</button></footer>
  </dialog>
}
