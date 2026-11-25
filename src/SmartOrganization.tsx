import {useEffect,useRef,useState} from "react";
import {invoke} from "@tauri-apps/api/core";
import {ArrowRight,Check,RefreshCw,Search,X} from "lucide-react";
import {isServiceCatalogPath} from "./prompt-folder-selection";

type Rules={catalog:boolean;format:string|null;destination:string|null};
type Suggestion={source:string;destination:string;target:string;bytes:number;modifiedMs:number|null;reason:string;workId:number|null;blocked:string|null};
type Preview={items:Suggestion[];total:number;warnings:string[]};
type Applied={moved:string[];errors:string[];notAttempted:number};
export function SmartOrganization({root,folders,onClose,onChanged}:{root:string;folders:string[];onClose:()=>void;onChanged:()=>Promise<void>}){
  const dialog=useRef<HTMLDialogElement>(null);
  const [rules,setRules]=useState<Rules>({catalog:true,format:null,destination:null});
  const [preview,setPreview]=useState<Preview|null>(null);
  const [selected,setSelected]=useState<Set<string>>(new Set());
  const [error,setError]=useState("");
  const [result,setResult]=useState<Applied|null>(null);
  const [busy,setBusy]=useState(false);
  const lock=useRef(false);
  const active=useRef(true);
  useEffect(()=>{active.current=true;dialog.current?.showModal();return()=>{active.current=false;};},[]);
  const change=(next:Rules)=>{setRules(next);setPreview(null);setSelected(new Set());setError("");setResult(null);};
  const allowed=preview?.items.filter(item=>!item.blocked)??[];
  const valid=(rules.catalog||rules.format!==null)&&(rules.format===null||rules.destination!==null);
  const run=async(apply:boolean)=>{
    if(lock.current||!valid)return;
    lock.current=true;setBusy(true);setError("");setResult(null);
    try{
      if(apply){
        const chosen=allowed.filter(item=>selected.has(item.source));
        const outcome=await invoke<Applied>("apply_smart_organization",{root,rules,selected:chosen});
        if(active.current){setResult(outcome);setPreview(null);setSelected(new Set());}
        // Even a failed move may require a rescan after a rollback attempt.
        await onChanged();
      }else{
        const outcome=await invoke<Preview>("preview_smart_organization",{root,rules});
        if(active.current){setPreview(outcome);setSelected(new Set());}
      }
    }catch(failure){if(active.current){setError(String(failure));setPreview(null);setSelected(new Set());}}
    finally{lock.current=false;if(active.current)setBusy(false);}
  };
  const selectableFolders=folders.filter(folder=>!isServiceCatalogPath(folder));
  return <dialog ref={dialog} className="download-picker glass-panel organize-modal smart-organization" aria-labelledby="smart-title" onCancel={e=>{e.preventDefault();if(!lock.current)onClose();}}>
    <div className="download-picker-header"><div><h2 id="smart-title">Organizar biblioteca</h2><p title={root}>{root}</p></div><button className="icon-button" disabled={busy} aria-label="Fechar" onClick={onClose}><X size={19}/></button></div>
    <div className="download-picker-body organization-body">
      <label className="organization-approval"><input type="checkbox" disabled={busy} checked={rules.catalog} onChange={e=>change({...rules,catalog:e.target.checked})}/> Usar destinos cadastrados no catálogo</label>
      <label>Regra para livros sem associação
        <select className="download-name-select" disabled={busy} value={rules.format??""} onChange={e=>change({...rules,format:e.target.value||null,destination:e.target.value?"":null})}>
          <option value="">Não aplicar</option>{["PDF","EPUB","MOBI","AZW3","DJVU"].map(format=><option key={format}>{format}</option>)}
        </select>
      </label>
      {rules.format&&<label>Pasta de destino<select className="download-name-select" value={rules.destination??""} disabled={busy} onChange={e=>change({...rules,destination:e.target.value})}><option value="">Raiz da biblioteca</option>{selectableFolders.map(folder=><option key={folder} value={folder}>{folder}</option>)}</select></label>}
      <button className="button secondary full" disabled={!valid||busy} onClick={()=>void run(false)}><Search size={16}/> Gerar prévia</button>
      {error&&<p role="alert" className="download-error">{error}</p>}
      {result&&<div role="status" className="organization-review"><strong>{result.moved.length} arquivo(s) movido(s)</strong>{!!result.moved.length&&<details><summary>Ver arquivos movidos</summary>{result.moved.map(path=><small key={path} title={path}>{path}</small>)}</details>}{result.errors.map(item=><p className="download-error" key={item}>{item}</p>)}{!!result.notAttempted&&<p>{result.notAttempted} movimentação(ões) não iniciada(s). Gere outra prévia.</p>}</div>}
      {preview&&<div className="organization-review">
        <div className="smart-review-head"><strong>{allowed.length} disponíveis · {preview.items.length-allowed.length} bloqueadas</strong><button className="button secondary" disabled={busy||!allowed.length} onClick={()=>setSelected(selected.size===allowed.length?new Set():new Set(allowed.map(item=>item.source)))}>{selected.size===allowed.length&&allowed.length?"Desmarcar todas":"Selecionar disponíveis"}</button></div>
        {preview.warnings.map(warning=><small key={warning}>{warning}</small>)}
        {!preview.items.length&&<p>Nenhuma movimentação sugerida.</p>}
        <div className="smart-moves">{preview.items.map(item=><label className="smart-move" key={item.source}>
          <input type="checkbox" disabled={busy||!!item.blocked} checked={selected.has(item.source)} onChange={e=>setSelected(old=>{const next=new Set(old);if(e.target.checked)next.add(item.source);else next.delete(item.source);return next;})}/>
          <span><small>{item.reason}</small><span className="smart-paths"><span title={item.source}>{item.source}</span><ArrowRight size={14}/><strong title={item.target}>{item.target}</strong></span>{item.blocked&&<small className="download-error">{item.blocked}</small>}</span>
        </label>)}</div>
      </div>}
    </div>
    <div className="download-picker-footer"><button className="button secondary" disabled={busy} onClick={onClose}>Fechar</button><button className="button primary" disabled={busy||!selected.size} onClick={()=>void run(true)}>{busy?<RefreshCw size={16} className="spinning"/>:<Check size={16}/>} Mover {selected.size} arquivo(s)</button></div>
  </dialog>;
}
