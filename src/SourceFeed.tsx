import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { BookOpen, ChevronRight, Download, Globe2, RefreshCw, Search } from "lucide-react";
import { groupDiscoveryWorks } from "./discovery-works";
import { mergeSourcePage, resultIdentity, type SearchOutcome, type SourceResult } from "./discovery-pagination";
import type { InstalledSource } from "./SourcesCenter";
import "./source-feed.css";

type Props = {
  source: InstalledSource;
  full?: boolean;
  onOpen: () => void;
  onWeb: () => void;
  onDownload: (item: SourceResult) => void;
};
export function SourceFeed({source,full=false,onOpen,onWeb,onDownload}:Props) {
  const {manifest,enabled}=source;
  const supported=!!manifest.feedPath || (manifest.adapter !== "opds" && manifest.adapter !== "html" && !!manifest.localSearch);
  const [outcome,setOutcome]=useState<SearchOutcome|null>(null);
  const [error,setError]=useState("");
  const [loading,setLoading]=useState(false);
  const [expanded,setExpanded]=useState<string|null>(null);
  const [visible,setVisible]=useState(24);
  const generation=useRef(0);
  const inFlight=useRef(false);
  const signature=JSON.stringify(manifest);
  const grouped=useMemo(()=>groupDiscoveryWorks(outcome?.results??[]),[outcome]);
  const shown=full?grouped.slice(0,visible):grouped.slice(0,12);
  const cursor=(outcome?.results.length??0)<400?outcome?.nextPages?.[manifest.id]:undefined;
  const load=async(next:string|null=null)=>{
    if(inFlight.current || !enabled || !supported)return;
    inFlight.current=true;
    const current=++generation.current;
    setLoading(true);setError("");
    if(!next){setOutcome(null);setExpanded(null);setVisible(24);}
    try{
      const page=await invoke<SearchOutcome>("browse_source",{id:manifest.id,cursor:next});
      if(generation.current!==current)return;
      if(page.errors.length)throw new Error(page.errors.join("; "));
      setOutcome(previous=>{
        const combined=next&&previous?mergeSourcePage(previous,page,manifest.id):page;
        return {...combined,results:combined.results.slice(0,400)};
      });
      if(next)setVisible(n=>n+40);
    }catch(failure){if(generation.current===current)setError(String(failure));}
    finally{if(generation.current===current){inFlight.current=false;setLoading(false);}}
  };
  useEffect(()=>{
    generation.current++;inFlight.current=false;setOutcome(null);setError("");setExpanded(null);setLoading(false);
    // Defer the automatic request so StrictMode's first setup/cleanup does not
    // issue duplicate calls. Explicit refresh remains immediate.
    const timer=window.setTimeout(()=>{if(enabled&&supported)void load();},0);
    return()=>{window.clearTimeout(timer);generation.current++;inFlight.current=false;};
  // The manifest identity also detects updates that change the endpoint or mapping.
  },[signature,enabled,supported]);
  return <section className={"source-feed glass-panel"+(full?" source-feed-full":"")}>
    <header className="source-feed-header"><div className="source-feed-title"><Globe2 size={19}/><div><h2>{manifest.name}</h2>{full&&manifest.description&&<small>{manifest.description}</small>}</div></div>
      <div className="source-feed-actions">
        <button className="button secondary" onClick={onWeb} title={manifest.websiteUrl||manifest.baseUrl}><Globe2 size={15}/> WebView</button>
        {supported&&enabled&&<button className="icon-button" disabled={loading} aria-label={"Atualizar feed de "+manifest.name} title="Atualizar feed" onClick={()=>void load()}><RefreshCw size={16} className={loading?"spinning":""}/></button>}
        {!full&&<button className="button secondary" onClick={onOpen}>Ver fonte <ChevronRight size={15}/></button>}
      </div>
    </header>
    {!enabled?<p className="feed-empty">Fonte desativada.</p>:!supported?<div className="feed-empty"><Search size={17}/> Esta fonte oferece pesquisa, mas não declara um feed.</div>:<>
      {error&&<div className="source-errors" role="alert">{error}<button className="button secondary" disabled={loading} onClick={()=>void load(outcome?cursor??null:null)}>Tentar novamente</button></div>}
      {loading&&!outcome&&<div className="feed-empty" role="status"><RefreshCw size={16} className="spinning"/> Carregando livros…</div>}
      {outcome&&!grouped.length&&<p className="feed-empty">Nenhum livro disponível neste feed.</p>}
      {!!shown.length&&<div className="feed-books">{shown.map(work=><article key={work.key} className="feed-book">
        <div className="feed-book-main"><div className="feed-book-icon"><BookOpen size={23}/></div><div className="feed-book-text"><strong title={work.title}>{work.title}</strong><small title={work.author}>{work.author||"Autor não informado"}</small><small>{[...new Set(work.editions.map(item=>item.format.toUpperCase()))].join(" · ")||"Metadados"}</small></div></div>
        {work.editions.length===1?<button className="button secondary" onClick={()=>onDownload(work.editions[0])}><Download size={14}/> Baixar</button>:work.editions.length>1?<button className="button secondary" aria-expanded={expanded===work.key} onClick={()=>setExpanded(old=>old===work.key?null:work.key)}>{work.editions.length} opções <ChevronRight size={14}/></button>:null}
        {expanded===work.key&&<div className="feed-editions">{work.editions.map(item=><button key={resultIdentity(item)} className="button secondary" onClick={()=>onDownload(item)}><Download size={14}/><span>{[item.format.toUpperCase(),item.language,item.publisher,item.editionLabel].filter(Boolean).join(" · ")}</span></button>)}</div>}
      </article>)}</div>}
      {full&&grouped.length>visible&&<button className="button secondary feed-more" onClick={()=>setVisible(n=>n+24)}>Mostrar mais</button>}
      {full&&(outcome?.results.length??0)>=400&&<small className="feed-empty">Limite de 400 resultados. Refine pela pesquisa.</small>}
      {full&&cursor&&<button className="button secondary feed-more" disabled={loading} onClick={()=>void load(cursor)}>{loading?"Carregando…":"Carregar próxima página"}</button>}
    </>}
  </section>;
}
