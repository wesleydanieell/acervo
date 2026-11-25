import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ask } from "@tauri-apps/plugin-dialog";
import { Download, Globe2, Plus, RefreshCw, Trash2, X } from "lucide-react";
import type { InstalledSource, SourceManifest } from "./SourcesCenter";
import "./extension-repositories.css";

type Entry = {id:string; name:string; version:string; manifestUrl:string; sha256:string};
type Index = {schemaVersion:number; name:string; extensions:Entry[]};
type Preview = {url:string; entry:Entry; manifest:SourceManifest};
const version = (v:string) => v.split(".").map(Number);
function newer(candidate:string, current:string) {
  const a=version(candidate), b=version(current);
  for(let i=0;i<3;i++){if(a[i]!==b[i]) return a[i]>b[i];}
  return false;
}

export function ExtensionRepositories({sources,onInstalled,disabled}:{sources:InstalledSource[];onInstalled:(items:InstalledSource[])=>void;disabled:boolean}) {
  const [urls,setUrls]=useState<string[]>([]);
  const [input,setInput]=useState("");
  const [selected,setSelected]=useState("");
  const [index,setIndex]=useState<Index|null>(null);
  const [preview,setPreview]=useState<Preview|null>(null);
  const [busy,setBusy]=useState(false);
  const [error,setError]=useState("");
  const [notice,setNotice]=useState("");
  const lock=useRef(false);
  const mounted=useRef(true);
  useEffect(()=>{mounted.current=true;void invoke<string[]>("list_repositories").then(items=>{if(mounted.current)setUrls(items);}).catch(e=>{if(mounted.current)setError(String(e));});return()=>{mounted.current=false;};},[]);
  const run=async(task:()=>Promise<void>)=>{
    if(lock.current || disabled)return;
    lock.current=true;setBusy(true);setError("");setNotice("");
    try{await task();}catch(e){if(mounted.current)setError(String(e));}
    finally{lock.current=false;if(mounted.current)setBusy(false);}
  };
  const browse=async(url:string)=>{
    setSelected(url);setIndex(null);setPreview(null);
    const value=await invoke<Index>("browse_repository",{url});
    if(mounted.current)setIndex(value);
  };
  const add=()=>run(async()=>{
    const items=await invoke<string[]>("add_repository",{url:input.trim()});
    if(!mounted.current)return;
    setUrls(items);setInput("");setNotice("Repositório adicionado.");
    const canonical=new URL(input.trim()).href;
    await browse(items.includes(canonical)?canonical:items[items.length-1]);
  });
  const remove=(url:string)=>run(async()=>{
    if(!await ask("Remover este repositório? As extensões instaladas serão preservadas.",{title:"Remover repositório",kind:"warning"}))return;
    const items=await invoke<string[]>("remove_repository",{url});
    if(!mounted.current)return;
    setUrls(items);
    if(selected===url){setSelected("");setIndex(null);setPreview(null);}
    setNotice("Repositório removido.");
  });
  const review=(entry:Entry)=>run(async()=>{
    setPreview(null);
    const manifest=await invoke<SourceManifest>("preview_repository_extension",{url:selected,id:entry.id,sha256:entry.sha256});
    if(mounted.current)setPreview({url:selected,entry,manifest});
  });
  const install=()=>run(async()=>{
    if(!preview)return;
    const items=await invoke<InstalledSource[]>("install_repository_extension",{url:preview.url,id:preview.entry.id,sha256:preview.entry.sha256});
    onInstalled(items);
    if(mounted.current){setPreview(null);setNotice("Extensão instalada ou atualizada.");}
  });
  const unavailable=busy||disabled;
  return <section className="repo-panel glass-panel">
    <div className="repo-heading"><h2><Globe2 size={18}/> Repositórios</h2></div>
    <form className="repo-add" onSubmit={e=>{e.preventDefault();void add();}}>
      <input type="url" aria-label="URL do índice de extensões" placeholder="https://…/index.json" value={input} maxLength={2047} onChange={e=>setInput(e.target.value)} required disabled={unavailable}/>
      <button type="submit" className="button primary" disabled={unavailable||!input.trim()}><Plus size={16}/> Adicionar</button>
    </form>
    {error&&<p className="download-error" role="alert">{error}</p>}
    {notice&&<p className="repo-notice" role="status">{notice}</p>}
    {urls.length===0?<p className="repo-muted">Nenhum repositório adicionado.</p>:<div className="repo-list">{urls.map(url=><div className="repo-row" key={url}>
      <button className="repo-url" aria-pressed={selected===url} disabled={unavailable} onClick={()=>void run(()=>browse(url))} title={url}>{url}</button>
      <button className="icon-button" title="Remover repositório" aria-label={"Remover "+url} disabled={unavailable} onClick={()=>void remove(url)}><Trash2 size={16}/></button>
    </div>)}</div>}
    {selected&&<div className="repo-heading"><h3>{index?.name??"Extensões disponíveis"}</h3><button className="button secondary" disabled={unavailable} onClick={()=>void run(()=>browse(selected))}><RefreshCw size={15}/> Atualizar</button></div>}
    {index&&<div className="repo-items">{index.extensions.map(entry=>{
      const local=sources.find(item=>item.manifest.id===entry.id);
      const update=local?.repository===selected&&newer(entry.version,local.manifest.version??"1.0.0");
      return <div className="repo-row repo-entry" key={entry.id}><div><strong>{entry.name}</strong><small>{entry.id} · v{entry.version}</small>{local&&<small>{local.repository!==selected?"Instalada de outra origem":update?"Atualização disponível":"Instalada"}</small>}</div>
      <button className="button secondary" disabled={unavailable||!!local&&!update} onClick={()=>void review(entry)}><Download size={15}/>{update?"Atualizar":"Instalar"}</button></div>;
    })}</div>}
    {preview&&<div className="repo-preview" role="region" aria-label="Revisar extensão">
      <div className="repo-heading"><h3>Revisar {preview.manifest.name}</h3><button className="icon-button" aria-label="Fechar revisão" disabled={unavailable} onClick={()=>setPreview(null)}><X size={17}/></button></div>
      <small>v{preview.manifest.version??"1.0.0"} · {(preview.manifest.adapter??"json").toUpperCase()} · API {preview.manifest.apiVersion??1}</small>
      <p>{preview.manifest.description}</p>
      <small>Origem de pesquisa: {preview.manifest.baseUrl}</small>
      {preview.manifest.websiteUrl && <small>Site: {preview.manifest.websiteUrl}</small>}
      <small>Domínios de download: {preview.manifest.downloadDomains?.join(", ")||"Nenhum"}</small>
      <small>SHA-256 verificado em relação ao índice. Isso não comprova a identidade do autor; confie apenas em repositórios conhecidos.</small>
      <button className="button primary" disabled={unavailable} onClick={()=>void install()}>Confirmar instalação / atualização</button>
    </div>}
    {busy&&<small role="status" className="repo-muted">Consultando repositório…</small>}
  </section>;
}
