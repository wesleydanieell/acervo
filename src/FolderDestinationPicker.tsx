import {useMemo,useState} from "react";
import {Check,ChevronRight,Folder,FolderOpen,Home,Search,X} from "lucide-react";
import {directPromptChildren,isServiceCatalogPath} from "./prompt-folder-selection";
const fold=(text:string)=>text.normalize("NFD").replace(/[\u0300-\u036f]/g,"").toLocaleLowerCase("pt-BR");
export function FolderDestinationPicker({folders,value,onChange,counts,disabled=false}:{folders:readonly string[];value:string;onChange:(path:string)=>void;counts:ReadonlyMap<string,number>;disabled?:boolean}){
  const [query,setQuery]=useState("");
  const [limit,setLimit]=useState(80);
  const valid=useMemo(()=>folders.filter(path=>!isServiceCatalogPath(path)),[folders]);
  const current=value&&valid.includes(value)?value:"";
  const needle=fold(query.trim());
  const entries=useMemo(()=> (needle?valid.filter(path=>fold(path).includes(needle)):directPromptChildren(valid,current)).sort((a,b)=>a.localeCompare(b,"pt-BR")),[valid,current,needle]);
  const parts=current.split("/").filter(Boolean);
  const navigate=(path:string)=>{onChange(path);setQuery("");setLimit(80);};
  return <section className="destination-picker" aria-label="Explorar destinos">
    <div className="destination-heading"><strong>Pasta de destino</strong><span>{current||"Raiz da biblioteca"}</span></div>
    <div className="destination-search"><Search size={15}/><input aria-label="Pesquisar destinos" value={query} disabled={disabled} placeholder="Buscar em toda a biblioteca…" onChange={event=>{setQuery(event.target.value);setLimit(80);}}/>{query&&<button type="button" aria-label="Limpar busca de destinos" disabled={disabled} onClick={()=>{setQuery("");setLimit(80);}}><X size={14}/></button>}</div>
    <nav className="destination-breadcrumbs" aria-label="Caminho do destino">
      <button type="button" disabled={disabled} onClick={()=>navigate("")}><Home size={15}/> Raiz</button>
      {parts.map((part,index)=><span key={index}><ChevronRight size={12}/><button type="button" disabled={disabled} onClick={()=>navigate(parts.slice(0,index+1).join("/"))}>{part}</button></span>)}
    </nav>
    <div className="destination-current"><FolderOpen size={18}/><span><strong>{parts.slice(-1)[0]||"Biblioteca"}</strong><small>{current||"Raiz da biblioteca"}</small></span><Check size={18}/></div>
    <div className="destination-list" aria-label="Pastas disponíveis">
      {entries.slice(0,limit).map(path=><button type="button" disabled={disabled} className="destination-row" key={path} onClick={()=>navigate(path)}><Folder size={18}/><span><strong>{needle?path:path.split("/").slice(-1)[0]}</strong><small>{counts.get(path)||0} livros no ramo</small></span><ChevronRight size={15}/></button>)}
      {!entries.length&&<div className="destination-empty">{needle?"Nenhum destino encontrado.":"Nenhuma subpasta. Este destino está selecionado."}</div>}
      {entries.length>limit&&<button type="button" className="destination-more" disabled={disabled} onClick={()=>setLimit(n=>n+80)}>Mostrar mais ({entries.length-limit})</button>}
    </div>
  </section>;
}
