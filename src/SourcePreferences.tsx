import { useEffect, useRef, useState } from "react";
import { Check, RotateCcw, X } from "lucide-react";
import type { InstalledSource } from "./SourcesCenter";

export type SourcePreferences = {
  globalSearch: boolean;
  showFeed: boolean;
  loadCovers: boolean;
  preferredLanguage: string;
  preferredFormat: string;
  preferredCategory: string | null;
};
export const defaultSourcePreferences: SourcePreferences = {
  globalSearch: true, showFeed: true, loadCovers: true,
  preferredLanguage: "", preferredFormat: "", preferredCategory: null,
};
const languageNames: Record<string,string> = { pt:"Português", en:"Inglês", es:"Espanhol", fr:"Francês" };
const formats = ["epub","pdf","mobi","azw3","djvu"];
export function SourcePreferencesDialog({source,busy,error,onClose,onSave}:{
  source:InstalledSource; busy:boolean; error:string;
  onClose:()=>void; onSave:(value:SourcePreferences)=>void;
}) {
  const dialog=useRef<HTMLDialogElement>(null);
  const [value,setValue]=useState<SourcePreferences>(()=>({...defaultSourcePreferences,...source.preferences}));
  const {manifest}=source;
  const canFilterLanguage=manifest.adapter==="opds"||(manifest.adapter==="html"?!!manifest.html?.languageSelector:!!(manifest.languageField||manifest.searchPath.includes("{language}")||manifest.editions?.languageField));
  const canFilterFormat=manifest.adapter==="opds"||(manifest.adapter==="html"?!!(manifest.html?.downloadSelector||manifest.downloadResolver):!!(manifest.formatField||manifest.searchPath.includes("{format}")||manifest.editions||manifest.downloadResolver||(manifest.downloadUrlField&&manifest.formats.length===1)));
  const languages=manifest.languages.length?manifest.languages:Object.keys(languageNames);
  const availableFormats=manifest.formats.length?formats.filter(fmt=>manifest.formats.some(item=>item.toLowerCase()===fmt)):formats;
  const set=<K extends keyof SourcePreferences>(key:K,next:SourcePreferences[K])=>setValue(old=>({...old,[key]:next}));
  useEffect(()=>{dialog.current?.showModal();},[]);
  return <dialog className="download-picker glass-panel source-preferences" ref={dialog} aria-labelledby="source-preferences-title" onCancel={event=>{event.preventDefault();if(!busy)onClose();}}>
    <div className="download-picker-header"><div><h2 id="source-preferences-title">Configurações da extensão</h2><p>{manifest.name}</p></div><button className="icon-button" disabled={busy} onClick={onClose} aria-label="Fechar"><X size={19}/></button></div>
    <form onSubmit={event=>{event.preventDefault();if(!busy)onSave(value);}}>
      <div className="download-picker-body source-preferences-body">
        <section><h3>Descobrir</h3>
          <label><input type="checkbox" checked={value.globalSearch} disabled={busy} onChange={event=>set("globalSearch",event.target.checked)}/> Incluir na pesquisa global</label>
          <label><input type="checkbox" checked={value.showFeed} disabled={busy} onChange={event=>set("showFeed",event.target.checked)}/> Mostrar feed na página inicial</label>
          <label><input type="checkbox" checked={value.loadCovers} disabled={busy} onChange={event=>set("loadCovers",event.target.checked)}/> Carregar capas</label>
          {!!manifest.feedCategories?.length && <label className="source-pref-select">Categoria inicial<select disabled={busy} value={value.preferredCategory??""} onChange={event=>set("preferredCategory",event.target.value||null)}><option value="">Padrão da fonte</option>{manifest.feedCategories.map(item=><option key={item.id} value={item.id}>{item.label}</option>)}</select></label>}
        </section>
        <section><h3>Pesquisa individual</h3>
          {canFilterLanguage&&<label className="source-pref-select">Idioma inicial<select disabled={busy} value={value.preferredLanguage} onChange={event=>set("preferredLanguage",event.target.value)}><option value="">Todos</option>{languages.map(item=><option key={item} value={item}>{languageNames[item]??item}</option>)}</select></label>}
          {canFilterFormat&&<label className="source-pref-select">Formato inicial<select disabled={busy} value={value.preferredFormat} onChange={event=>set("preferredFormat",event.target.value)}><option value="">Todos</option>{availableFormats.map(item=><option key={item} value={item}>{item.toUpperCase()}</option>)}</select></label>}
          {!canFilterLanguage&&!canFilterFormat&&<small>Esta fonte não oferece filtros de idioma ou formato.</small>}
        </section>
        {error&&<p className="download-error" role="alert">{error}</p>}
      </div>
      <div className="download-picker-footer"><button type="button" className="button secondary" disabled={busy} onClick={()=>setValue({...defaultSourcePreferences})}><RotateCcw size={15}/> Restaurar padrões</button><button type="submit" className="button primary" disabled={busy}><Check size={16}/>{busy?"Salvando…":"Salvar"}</button></div>
    </form>
  </dialog>;
}
