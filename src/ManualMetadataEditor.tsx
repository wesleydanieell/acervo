import {useEffect,useRef,useState} from "react";
import {invoke} from "@tauri-apps/api/core";
import {LoaderCircle,Save,X} from "lucide-react";
import {inheritedMetadata,metadataFields,prepareManualMetadata,type MetadataFile,type ManualMetadata} from "./manual-metadata";

type Snapshot={metadata:ManualMetadata;expected:{identity:unknown;revision:string;path:string;root:string}};
export function ManualMetadataEditor({root,file,onClose,onChanged}:{root:string;file:MetadataFile&{path:string};onClose:()=>void;onChanged:()=>Promise<void>}){
  const dialog=useRef<HTMLDialogElement>(null);
  const lock=useRef(false);
  const active=useRef(true);
  const [snapshot,setSnapshot]=useState<Snapshot|null>(null);
  const [fields,setFields]=useState<ManualMetadata>({});
  const [loading,setLoading]=useState(true);
  const [busy,setBusy]=useState(false);
  const [error,setError]=useState("");
  const fallback=inheritedMetadata(file);
  useEffect(()=>{
    active.current=true;dialog.current?.showModal();
    let cancelled=false;
    void invoke<Snapshot>("get_manual_book_metadata",{root,path:file.path}).then(value=>{
      if(!cancelled){setSnapshot(value);setFields(value.metadata??{});}
    }).catch(failure=>{if(!cancelled)setError(String(failure));}).finally(()=>{if(!cancelled)setLoading(false);});
    return()=>{cancelled=true;active.current=false;};
  },[root,file.path]);
  const save=async()=>{
    if(lock.current||!snapshot)return;
    lock.current=true;setBusy(true);setError("");
    try{
      const metadata=prepareManualMetadata(fields);
      await invoke("save_manual_book_metadata",{root,path:file.path,expected:snapshot.expected,metadata});
      if(active.current){await onChanged();onClose();}
    }catch(failure){if(active.current)setError(String(failure));}
    finally{lock.current=false;if(active.current)setBusy(false);}
  };
  return <dialog className="download-picker glass-panel manual-metadata-editor" ref={dialog} aria-labelledby="manual-metadata-title" onCancel={event=>{event.preventDefault();if(!lock.current)onClose();}}>
    <div className="download-picker-header"><div><h2 id="manual-metadata-title">Editar metadados</h2><p title={file.path}>{file.path}</p></div><button className="icon-button" aria-label="Fechar" disabled={busy} onClick={onClose}><X size={19}/></button></div>
    <div className="download-picker-body organization-body">
      {loading&&<small role="status"><LoaderCircle size={16} className="spinning"/> Carregando…</small>}
      {snapshot&&<>
        {metadataFields.map(([key,label,max])=><label key={key}>{label}{key==="description"?<textarea value={fields[key]??""} maxLength={max} rows={3} disabled={busy} placeholder={fallback[key]||"Não informado"} onChange={e=>setFields(old=>({...old,[key]:e.target.value}))}/>:<input value={fields[key]??""} maxLength={max} disabled={busy} placeholder={fallback[key]||"Não informado"} onChange={e=>setFields(old=>({...old,[key]:e.target.value}))}/>}</label>)}
        <small>Campos vazios usam os metadados originais. O arquivo do livro não será modificado.</small>
        <button className="button secondary" disabled={busy} onClick={()=>setFields({})}>Restaurar campos originais</button>
      </>}
      {error&&<p className="download-error" role="alert">{error}</p>}
    </div>
    <div className="download-picker-footer"><button className="button secondary" disabled={busy} onClick={onClose}>Cancelar</button><button className="button primary" disabled={loading||busy||!snapshot} onClick={()=>void save()}>{busy?<LoaderCircle size={16} className="spinning"/>:<Save size={16}/>} Salvar metadados</button></div>
  </dialog>;
}
