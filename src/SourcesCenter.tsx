import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { Check, CircleHelp, ClipboardCopy, Download, ExternalLink, FileJson2, Globe2, Plus, RefreshCw, Search, ShieldCheck, ToggleLeft, ToggleRight, Trash2, X } from "lucide-react";
import "./sources.css";

export type SourceManifest = {
  schemaVersion: number; id: string; name: string; description: string;
  baseUrl: string; searchPath: string; itemsPath: string; titleField: string;
  authorField: string; formatField: string; languageField: string; downloadUrlField: string;
  formats: string[]; languages: string[];
};
export type InstalledSource = { manifest: SourceManifest; enabled: boolean };

const sampleManifest = JSON.stringify({
  schemaVersion: 1, id: "minha-fonte", name: "Minha fonte", description: "Catálogo JSON público",
  baseUrl: "https://example.org", searchPath: "/api/books?q={query}&lang={language}",
  itemsPath: "results", titleField: "title", authorField: "author",
  formatField: "format", languageField: "language", downloadUrlField: "download_url",
  formats: ["epub", "pdf"], languages: ["pt", "en"],
}, null, 2);

export function SourcesCenter() {
  const [sources, setSources] = useState<InstalledSource[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [pendingRemoval, setPendingRemoval] = useState("");
  const [showExample, setShowExample] = useState(false);

  const reload = async () => {
    try { setSources(await invoke<InstalledSource[]>("list_sources")); setError(""); }
    catch (failure) { setError(String(failure)); }
  };
  useEffect(() => { void reload(); }, []);

  const install = async () => {
    setError(""); setNotice("");
    try {
      const file = await open({ multiple: false, directory: false, title: "Instalar fonte JSON",
        filters: [{ name: "Manifesto de fonte", extensions: ["json"] }] });
      if (typeof file !== "string") return;
      setBusy(true);
      const installed = await invoke<InstalledSource[]>("install_source", { path: file });
      setSources(installed);
      setNotice("Fonte instalada. Ela já pode ser habilitada ou desabilitada.");
    } catch (failure) { setError(String(failure)); }
    finally { setBusy(false); }
  };
  const toggle = async (id: string, enabled: boolean) => {
    setBusy(true); setError("");
    try { setSources(await invoke<InstalledSource[]>("set_source_enabled", { id, enabled })); }
    catch (failure) { setError(String(failure)); }
    finally { setBusy(false); }
  };
  const remove = async (id: string) => {
    setBusy(true); setError("");
    try {
      setSources(await invoke<InstalledSource[]>("remove_source", { id }));
      setPendingRemoval("");
      setNotice("Fonte removida. Nenhum livro ou arquivo baixado foi apagado.");
    } catch (failure) { setError(String(failure)); }
    finally { setBusy(false); }
  };
  const copyExample = async () => {
    try { await navigator.clipboard.writeText(sampleManifest); setNotice("Manifesto de exemplo copiado."); }
    catch (failure) { setError("Falha ao copiar: " + String(failure)); }
  };

  return <section className="sources-center">
    <div className="page-intro">
      <div><div className="eyebrow">FONTES E EXTENSÕES</div><h1>Fontes da biblioteca</h1>
        <p>Instale manifestos JSON e escolha quais fontes serão usadas na pesquisa de livros.</p></div>
      <div className="sources-top-actions"><button className="button secondary" disabled={busy} onClick={() => void reload()}><RefreshCw size={17}/> Atualizar</button>
        <button className="button primary" disabled={busy} onClick={() => void install()}><Plus size={17}/> Instalar fonte</button></div>
    </div>
    {error && <div className="alert" role="alert"><CircleHelp size={18}/><span>{error}</span><button className="icon-button" onClick={() => setError("")} aria-label="Fechar alerta"><X size={16}/></button></div>}
    {notice && <div className="inline-status" role="status"><Check size={16}/>{notice}</div>}
    <div className="sources-summary glass-panel">
      <div className="sources-summary-icon"><ShieldCheck size={25}/></div>
      <div><strong>Extensões declarativas e isoladas</strong>
        <p>Arquivos de configuração, não programas executáveis. Cada fonte declara seu domínio HTTPS e como ler respostas JSON. Nenhuma fonte recebe acesso direto à sua biblioteca.</p></div>
      <span>{sources.filter((source) => source.enabled).length} ativa(s)</span>
    </div>
    {sources.length ? <div className="source-list">{sources.map(({ manifest, enabled }) => <article className="source-card glass-panel" key={manifest.id}>
      <div className="source-card-main"><div className="source-icon"><Globe2 size={24}/></div><div className="source-info">
        <strong>{manifest.name}</strong><small>{manifest.description || manifest.baseUrl}</small>
        <code title={manifest.baseUrl}>{manifest.baseUrl}</code>
      </div><span className={"source-state " + (enabled ? "enabled" : "")}>{enabled ? "Ativa" : "Pausada"}</span></div>
      <div className="source-capabilities">
        {manifest.formats.map((format) => <span key={"f:"+format}>{format.toUpperCase()}</span>)}
        {manifest.languages.map((language) => <span key={"l:"+language}>{language}</span>)}
        <span><Search size={13}/> Busca JSON</span>
        {manifest.downloadUrlField && <span><Download size={13}/> Link de download</span>}
      </div>
      <div className="source-actions">
        <button className="button secondary" disabled={busy} onClick={() => void toggle(manifest.id, !enabled)}>
          {enabled ? <ToggleRight size={17}/> : <ToggleLeft size={17}/>} {enabled ? "Desativar" : "Ativar"}</button>
        {pendingRemoval === manifest.id ? <div className="source-remove-confirm"><span>Remover esta fonte?</span>
          <button className="button secondary" onClick={() => setPendingRemoval("")}>Cancelar</button>
          <button className="button danger" disabled={busy} onClick={() => void remove(manifest.id)}>Remover</button></div>
          : <button className="button secondary" onClick={() => setPendingRemoval(manifest.id)}><Trash2 size={16}/> Remover</button>}
      </div>
    </article>)}</div> :
      <div className="empty-state glass-panel source-empty"><FileJson2 size={37}/>
        <h3>Nenhuma fonte instalada</h3>
        <p>Instale um manifesto JSON para registrar uma fonte. Não executamos scripts de terceiros.</p>
        <button className="button primary" onClick={() => void install()}><Plus size={17}/> Selecionar arquivo JSON</button>
      </div>}
    <section className="source-help glass-panel">
      <div><h2>Formato de extensão v1</h2>
        <p>Uma fonte descreve um endpoint público HTTPS e os campos JSON para títulos, autores, idiomas e formatos. O motor de pesquisa e a fila de downloads serão conectados a estes manifestos na próxima etapa.</p></div>
      <div className="source-help-actions">
        <button className="button secondary" onClick={() => setShowExample((current) => !current)}><FileJson2 size={16}/> {showExample ? "Ocultar exemplo" : "Ver exemplo"}</button>
        <button className="button secondary" onClick={() => void copyExample()}><ClipboardCopy size={16}/> Copiar JSON</button>
      </div>
      {showExample && <pre className="source-example">{sampleManifest}</pre>}
      <p className="source-disclaimer"><ExternalLink size={14}/> Instale apenas fontes de confiança e respeite as condições de distribuição dos arquivos.</p>
    </section>
  </section>;
}
