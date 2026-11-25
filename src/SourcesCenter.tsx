import { mergeSourcePage, resultIdentity, type SourceResult, type SearchOutcome } from "./discovery-pagination";
import { groupDiscoveryWorks } from "./discovery-works";
import { SourceCover } from "./SourceCover";
import { SourceFeed } from "./SourceFeed";
import { ExtensionRepositories } from "./ExtensionRepositories";
import { DownloadCatalogPicker } from "./DownloadCatalogPicker";
import type { CatalogItem } from "./CatalogCenter";
import type { CatalogTarget, DiscoveryRequest } from "./download-workspace";
import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import { ArrowLeft, Check, CircleHelp, ClipboardCopy, Download, ExternalLink, FileJson2, Globe2, Plus, RefreshCw, Search, ToggleLeft, ToggleRight, Trash2, X } from "lucide-react";
import "./sources.css";
import "./source-search.css";

export type SourceManifest = {
  schemaVersion: number; apiVersion?: number; version?: string; adapter?: "json" | "opds" | "html"; id: string; name: string; description: string;
  coverUrlField?: string;
  websiteUrl?: string | null; feedPath?: string | null; localSearch?: boolean;
  baseUrl: string; searchPath: string; itemsPath: string; titleField: string;
  authorField: string; formatField: string; languageField: string; downloadUrlField: string;
  formats: string[]; languages: string[];
  html?: { itemSelector: string; titleSelector: string; downloadSelector?: string; nextSelector?: string; coverSelector?: string } | null;
  editions?: {itemsField:string;formatField:string;downloadUrlField:string;languageField?:string;publisherField?:string;labelField?:string} | null;
  remoteIdField?: string; pagination?: { initialCursor: string; nextCursorField: string } | null; downloadDomains?: string[] | null;
};
export type InstalledSource = { manifest: SourceManifest; enabled: boolean; repository?: string | null; manifestSha256?: string | null };

const sampleManifest = JSON.stringify({
  schemaVersion: 1, apiVersion: 1, version: "1.0.0", adapter: "json", id: "minha-fonte", name: "Minha fonte", description: "Catálogo JSON público",
  baseUrl: "https://example.org", searchPath: "/api/books?q={query}&lang={language}",
  itemsPath: "results", titleField: "title", authorField: "author",
  formatField: "format", languageField: "language", downloadUrlField: "download_url",
  formats: ["epub", "pdf"], languages: ["pt", "en"],
}, null, 2);

export function SourcesCenter({ mode, initialSearch, onClearRequest, onShowExtensions, onShowDiscover, onShowDownloads, root, works }: {
  mode: "discover" | "extensions";
  root: string; works: CatalogItem[];
  initialSearch?: DiscoveryRequest | null;
  onClearRequest: () => void;
  onShowExtensions: () => void;
  onShowDiscover: () => void;
  onShowDownloads: () => void;
}) {
  const [downloadChoice, setDownloadChoice] = useState<SourceResult | null>(null);
  const [queueError, setQueueError] = useState("");
  const [sources, setSources] = useState<InstalledSource[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [pendingRemoval, setPendingRemoval] = useState("");
  const [showExample, setShowExample] = useState(false);
  const [query, setQuery] = useState(initialSearch?.query ?? "");
  const [language, setLanguage] = useState(initialSearch?.language ?? "");
  const [format, setFormat] = useState(initialSearch?.format ?? "");
  const [sourceId, setSourceId] = useState("");
  const [sourceDetail,setSourceDetail]=useState("");
  const [visibleFeeds,setVisibleFeeds]=useState(4);
  const [searching, setSearching] = useState(false);
  const [loadingPage, setLoadingPage] = useState("");
  const pageLock = useRef(false);
  const [searchOutcome, setSearchOutcome] = useState<SearchOutcome | null>(null);
  const [expandedWorks, setExpandedWorks] = useState<Set<string>>(new Set());
  const groupedWorks = useMemo(() => groupDiscoveryWorks(searchOutcome?.results ?? []), [searchOutcome]);
  const [queued, setQueued] = useState<Set<string>>(new Set());
  const queuePending = useRef(new Set<string>());
  const [pending, setPending] = useState<Set<string>>(new Set());
  const searchGeneration = useRef(0);
  const activeSources = sources.filter(item => item.enabled);
  const discover = mode === "discover";
  const detailSource=sources.find(item=>item.manifest.id===sourceDetail);
  const feedSources=sourceId ? activeSources.filter(item=>item.manifest.id===sourceId) : activeSources;
  const openDetail=(id:string)=>{
    setSourceDetail(id);setSourceId(id);setQuery("");setLanguage("");setFormat("");invalidateSearch();onClearRequest();onShowDiscover();
  };
  const leaveDetail=()=>{
    setSourceDetail("");setSourceId("");setQuery("");setLanguage("");setFormat("");invalidateSearch();onClearRequest();
  };
  const chooseEdition=(item:SourceResult)=>{setQueueError("");setDownloadChoice(item);};
  const resultKey = (item: SourceResult) => JSON.stringify([resultIdentity(item), initialSearch?.destination ?? "", initialSearch?.catalogTarget ?? null]);
  const invalidateSearch = () => { searchGeneration.current++; setSearching(false); setLoadingPage(""); pageLock.current = false; setSearchOutcome(null); setExpandedWorks(new Set()); setQueued(new Set()); };
  useEffect(() => {
    if (sourceId && !sourceDetail && !sources.some(item => item.enabled && item.manifest.id === sourceId)) {
      setSourceId(""); invalidateSearch();
    }
  }, [sources, sourceId, sourceDetail]);
  useEffect(() => {
    if (!initialSearch) return;
    setSourceDetail("");
    setQuery(initialSearch.query); setLanguage(initialSearch.language); setFormat(initialSearch.format);
    setSourceId(""); invalidateSearch();
  }, [initialSearch]);

  useEffect(()=>{
    if(sourceDetail && !sources.some(item=>item.manifest.id===sourceDetail)){setSourceDetail("");setSourceId("");invalidateSearch();}
  },[sources,sourceDetail]);
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
  const installDemo = async () => {
    setBusy(true); setError(""); setNotice("");
    try { setSources(await invoke<InstalledSource[]>("install_demo_source"));
      setNotice("Open Library adicionada como exemplo opcional de fonte de metadados.");
    } catch (failure) { setError(String(failure)); }
    finally { setBusy(false); }
  };
  const installExample = async (id: string) => {
    setBusy(true); setError(""); setNotice("");
    try { setSources(await invoke<InstalledSource[]>("install_example_source", { id })); setNotice("Fonte instalada."); }
    catch (failure) { setError(String(failure)); }
    finally { setBusy(false); }
  };
  const toggle = async (id: string, enabled: boolean) => {
    setBusy(true); setError("");
    try { setSources(await invoke<InstalledSource[]>("set_source_enabled", { id, enabled })); invalidateSearch(); }
    catch (failure) { setError(String(failure)); }
    finally { setBusy(false); }
  };
  const remove = async (id: string) => {
    setBusy(true); setError("");
    try {
      setSources(await invoke<InstalledSource[]>("remove_source", { id })); invalidateSearch();
      setPendingRemoval("");
      setNotice("Fonte removida. Nenhum livro ou arquivo baixado foi apagado.");
    } catch (failure) { setError(String(failure)); }
    finally { setBusy(false); }
  };
  const openSourceWebview = async (id: string) => {
    try { await invoke("open_source_webview", {id}); }
    catch (failure) { setError(String(failure)); }
  };
  const copyExample = async () => {
    try { await navigator.clipboard.writeText(sampleManifest); setNotice("Manifesto de exemplo copiado."); }
    catch (failure) { setError("Falha ao copiar: " + String(failure)); }
  };

  const queue = async (item: SourceResult, target: CatalogTarget | null, autoImport: boolean, filenameSuffix: string) => {
    const key = resultKey(item);
    if (queuePending.current.has(key)) return;
    queuePending.current.add(key);
    setPending(new Set(queuePending.current));
    setQueueError(""); setError(""); setNotice("");
    try {
      await invoke("queue_download", {
        url: item.downloadUrl, title: item.title, sourceId: item.sourceId,
        edition: {title:item.title,author:item.author,language:item.language,publisher:item.publisher??"",editionLabel:item.editionLabel??"",remoteId:item.remoteId??null},
        filenameSuffix,
        format: item.format, plannedDestination: target?.destination === "." ? "" : target?.destination ?? "", catalogTarget: target, autoImport,
      });
      setQueued(previous => new Set(previous).add(key));
      setNotice(autoImport ? "Download adicionado. A importação ocorrerá ao concluir." : "Livro adicionado à fila.");
      setDownloadChoice(null);
    } catch (failure) { setQueueError(String(failure)); }
    finally { queuePending.current.delete(key); setPending(new Set(queuePending.current)); }
  };

  const search = async () => {
    if (detailSource && !detailSource.enabled) return;
    const generation = ++searchGeneration.current;
    setSearching(true); setLoadingPage(""); pageLock.current = false; setError(""); setSearchOutcome(null); setExpandedWorks(new Set()); setQueued(new Set());
    try {
      const outcome = await invoke<SearchOutcome>("search_sources", {
        query: query.trim(), language, format, sourceIds: sourceDetail ? [sourceDetail] : sourceId ? [sourceId] : [], cursor: null,
      });
      if (generation === searchGeneration.current) setSearchOutcome(outcome);
    } catch (failure) { if (generation === searchGeneration.current) setError(String(failure)); }
    finally { if (generation === searchGeneration.current) setSearching(false); }
  };

  const loadNext = async (id: string, cursor: string) => {
    if (pageLock.current || searching) return;
    pageLock.current = true;
    const generation = searchGeneration.current;
    setLoadingPage(id); setError("");
    try {
      const outcome = await invoke<SearchOutcome>("search_sources", { query: query.trim(), language, format, sourceIds: [id], cursor });
      if (generation === searchGeneration.current) setSearchOutcome(previous => previous ? mergeSourcePage(previous, outcome, id) : previous);
    } catch (failure) { if (generation === searchGeneration.current) setError(String(failure)); }
    finally { if (generation === searchGeneration.current) { pageLock.current = false; setLoadingPage(""); } }
  };

  return <section className="sources-center">
    <div className="page-intro">
      <div><h1>{discover ? detailSource?.manifest.name ?? "Descobrir" : "Extensões"}</h1>
        <p>{discover ? activeSources.length + " fontes ativas" : sources.length + " fontes instaladas"}</p></div>
      <div className="sources-top-actions">
        {discover ? <>{detailSource&&<button className="button secondary" onClick={leaveDetail}><ArrowLeft size={16}/> Todas as fontes</button>}<button className="button secondary" onClick={onShowExtensions}><Globe2 size={16}/> Extensões</button><button className="button secondary" onClick={onShowDownloads}><Download size={16}/> Downloads</button></> :
          <><button className="button secondary" onClick={()=>{leaveDetail();onShowDiscover();}}><Search size={16}/> Descobrir</button><button className="button secondary" disabled={busy} onClick={() => void reload()}><RefreshCw size={17}/> Atualizar</button>
          <button className="button secondary" disabled={busy || sources.some(item => item.manifest.id === "open-library")} onClick={() => void installDemo()}><Globe2 size={16}/> Open Library</button>
          <button className="button secondary" disabled={busy || sources.some(item => item.manifest.id === "gutenberg-curated")} onClick={() => void installExample("gutenberg-curated")}><Plus size={16}/> Gutenberg</button>
          <button className="button secondary" disabled={busy || sources.some(item => item.manifest.id === "acervo-test-source")} onClick={() => void installExample("acervo-test-source")}><Plus size={16}/> Fonte de teste</button>
          <button className="button primary" disabled={busy} onClick={() => void install()}><Plus size={17}/> Instalar JSON</button></>}
      </div>
    </div>
    {error && <div className="alert" role="alert"><CircleHelp size={18}/><span>{error}</span><button className="icon-button" onClick={() => setError("")} aria-label="Fechar alerta"><X size={16}/></button></div>}
    {notice && <div className="inline-status" role="status"><Check size={16}/>{notice}</div>}
    {discover ? <>
    {!activeSources.length && !detailSource && <div className="source-no-active glass-panel"><Globe2 size={20}/><span>Nenhuma fonte ativa</span><button className="button primary" onClick={onShowExtensions}>Gerenciar extensões</button></div>}
    <section className="source-search glass-panel">
      <div className="source-search-heading"><h2>Pesquisar livros</h2></div>
      {initialSearch && <div className="source-search-from-catalog"><span>{initialSearch.catalogTarget ? <>Obra: <strong>{initialSearch.catalogTarget.title}</strong> · </> : null}Destino: <strong>{initialSearch.destination || "Raiz"}</strong></span><button className="icon-button small" aria-label="Desvincular destino planejado" title="Desvincular destino planejado" onClick={onClearRequest}><X size={15}/></button></div>}
      <form className="source-search-form" onSubmit={(event) => { event.preventDefault(); void search(); }}>
        <label className="source-query"><span>Título ou autor</span><input value={query} onChange={(event) => { setQuery(event.target.value); invalidateSearch(); if (initialSearch) onClearRequest(); }} minLength={2} maxLength={200} placeholder="Ex.: Machado de Assis" aria-label="Buscar obras nas fontes" required/></label>
        <label><span>Fonte</span><select aria-label="Filtrar fonte" value={sourceDetail || sourceId} disabled={!!detailSource} onChange={(event) => { setSourceId(event.target.value); invalidateSearch(); }}><option value="">Todas as ativas</option>{sources.filter((item) => item.enabled || item.manifest.id===sourceDetail).map((item) => <option key={item.manifest.id} value={item.manifest.id}>{item.manifest.name}</option>)}</select></label>
        <label><span>Idioma</span><select aria-label="Filtrar idioma" value={language} onChange={(event) => { setLanguage(event.target.value); invalidateSearch(); }}><option value="">Todos</option><option value="pt">Português</option><option value="en">Inglês</option><option value="es">Espanhol</option><option value="fr">Francês</option></select></label>
        <label><span>Formato</span><select aria-label="Filtrar formato" value={format} onChange={(event) => { setFormat(event.target.value); invalidateSearch(); }}><option value="">Todos</option><option value="epub">EPUB</option><option value="pdf">PDF</option><option value="mobi">MOBI</option><option value="azw3">AZW3</option><option value="djvu">DJVU</option></select></label>
        <button className="button primary" type="submit" disabled={searching || !!loadingPage || query.trim().length < 2 || (!detailSource ? !sources.some((item) => item.enabled) : !detailSource.enabled)}>{searching ? <RefreshCw className="spinning" size={17}/> : <Search size={17}/>} Buscar</button>
      </form>
      {searchOutcome && <div className="source-results"><div className="source-results-heading"><strong>{groupedWorks.length} obra(s)</strong><small>{groupedWorks.reduce((sum,work)=>sum+work.editions.length,0)} opções de download</small></div>
        {searchOutcome.errors.length > 0 && <div className="source-errors" role="status">{searchOutcome.errors.map((text, index) => <p key={index}>{text}</p>)}</div>}
        {groupedWorks.length ? <div className="source-results-list">{groupedWorks.map(work => {
          const expanded = expandedWorks.has(work.key);
          const multiple = work.editions.length > 1;
          const shown = multiple && !expanded ? [] : work.editions;
          return <article key={work.key} className="source-work">
            <div className="source-result source-work-heading"><div className="source-result-book"><SourceCover sourceId={work.sourceId} url={[...work.editions,...work.metadata].find(item=>item.coverUrl)?.coverUrl} title={work.title}/><div><strong>{work.title}</strong><small>{work.author || "Autor não informado"} · {work.sourceName}</small></div></div>
              <div className="source-result-actions">{[...new Set(work.editions.map(item=>item.format.toUpperCase()))].map(fmt=><span key={fmt}>{fmt}</span>)}
                {multiple && <button className="button secondary" aria-expanded={expanded} onClick={()=>setExpandedWorks(previous=>{const next=new Set(previous);if(next.has(work.key))next.delete(work.key);else next.add(work.key);return next;})}>{expanded?"Ocultar opções":work.editions.length+" edições / formatos"}</button>}
                {!work.editions.length && <small>Somente metadados</small>}
                {work.metadata.filter(item=>item.downloadUrl).slice(0,1).map(item=><button key={resultIdentity(item)} className="button secondary" type="button" onClick={()=>void openUrl(item.downloadUrl).catch(failure=>setError(String(failure)))}><ExternalLink size={15}/> Link</button>)}
              </div></div>
            {shown.map((item,index)=><div className="source-edition" key={resultIdentity(item)+":"+index}><div><strong>{item.editionLabel || item.format.toUpperCase()}</strong><small>{[item.editionLabel && item.format.toUpperCase(),item.language,item.publisher].filter(Boolean).join(" · ") || "Idioma não informado"}</small></div>
              <div className="source-result-actions"><button className="button primary" type="button" disabled={pending.has(resultKey(item))} onClick={()=>{setQueueError("");setDownloadChoice(item);}}>{queued.has(resultKey(item))?<Check size={15}/>:<Download size={15}/>} {pending.has(resultKey(item))?"Adicionando…":queued.has(resultKey(item))?"Baixar novamente":"Baixar"}</button>
                <button className="button secondary" type="button" title="Abrir link externo informado pela fonte" onClick={()=>void openUrl(item.downloadUrl).catch(failure=>setError(String(failure)))}><ExternalLink size={15}/> Link</button></div></div>)}
          </article>;
        })}</div> : <p className="source-no-results">Nenhuma obra encontrada. Experimente outro termo, idioma ou fonte.</p>}
        {Object.entries(searchOutcome.nextPages ?? {}).length > 0 && <div className="source-pagination">{Object.entries(searchOutcome.nextPages ?? {}).map(([id, cursor]) =>
          <button key={id} className="button secondary" disabled={!!loadingPage || searching} onClick={() => void loadNext(id, cursor)}>
            {loadingPage === id && <RefreshCw className="spinning" size={14}/>} Mais resultados · {sources.find(s => s.manifest.id === id)?.manifest.name ?? id}
          </button>)}</div>}
      </div>}
    </section>
    {!query.trim() && !searchOutcome && !initialSearch && !searching && <div className="source-feed-stack">
      {detailSource ? <SourceFeed key={detailSource.manifest.id} source={detailSource} full onOpen={()=>{}} onWeb={()=>void openSourceWebview(detailSource.manifest.id)} onDownload={chooseEdition}/>
        : <><div className="source-feed-heading"><h2>Feeds das extensões</h2></div>
          {feedSources.slice(0,visibleFeeds).map(item=><SourceFeed key={item.manifest.id} source={item} onOpen={()=>openDetail(item.manifest.id)} onWeb={()=>void openSourceWebview(item.manifest.id)} onDownload={chooseEdition}/>)}
          {feedSources.length>visibleFeeds&&<button className="button secondary feed-more" onClick={()=>setVisibleFeeds(n=>n+4)}>Mostrar mais fontes</button>}
        </>}
    </div>}
    </> : <>
    <ExtensionRepositories sources={sources} disabled={busy} onInstalled={items => { setSources(items); invalidateSearch(); }}/>
    {sources.length ? <div className="source-list">{sources.map(({ manifest, enabled, repository }) => <article className="source-card glass-panel" key={manifest.id}>
      <div className="source-card-main"><div className="source-icon"><Globe2 size={24}/></div><div className="source-info">
        <button className="source-info-button" onClick={()=>openDetail(manifest.id)}><strong>{manifest.name}</strong></button><small>{manifest.description || manifest.baseUrl}</small><small>v{manifest.version ?? "1.0.0"} · API {manifest.apiVersion ?? 1}</small>
        <code title={manifest.baseUrl}>{manifest.baseUrl}</code>
        {repository && <small title={repository}>Instalada via repositório</small>}
      </div><span className={"source-state " + (enabled ? "enabled" : "")}>{enabled ? "Ativa" : "Pausada"}</span></div>
      <div className="source-capabilities">
        {manifest.formats.map((format) => <span key={"f:"+format}>{format.toUpperCase()}</span>)}
        {manifest.languages.map((language) => <span key={"l:"+language}>{language}</span>)}
        <span><Search size={13}/> Busca {manifest.adapter === "opds" ? "OPDS" : manifest.adapter === "html" ? "HTML" : "JSON"}</span>
        {(manifest.adapter === "opds" || manifest.downloadUrlField || manifest.editions || manifest.html?.downloadSelector) && (manifest.downloadDomains == null || manifest.downloadDomains.length > 0) && <span><Download size={13}/> Link de download</span>}
        {(manifest.adapter === "opds" || manifest.pagination || manifest.html?.nextSelector) && <span>Paginação</span>}
        {manifest.editions && <span>Múltiplas edições</span>}
        {(manifest.feedPath || manifest.localSearch) && <span>Feed</span>}
        {(manifest.adapter === "opds" || manifest.downloadUrlField || manifest.editions || manifest.html?.downloadSelector) && <span title={manifest.downloadDomains?.join(", ") || "Política de HTTPS público"}>{manifest.downloadDomains == null ? "Domínios: legado" : manifest.downloadDomains.length ? "Domínios: " + manifest.downloadDomains.join(", ") : "Downloads bloqueados"}</span>}
      </div>
      <div className="source-actions">
        <button className="button primary" onClick={()=>openDetail(manifest.id)}><Search size={16}/> Abrir fonte</button>
        <button className="button secondary" onClick={() => void openSourceWebview(manifest.id)} title={manifest.websiteUrl || manifest.baseUrl}><Globe2 size={16}/> WebView</button>
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
        <p>Instale um manifesto JSON ou experimente a fonte opcional Open Library (metadados). Não executamos scripts de terceiros.</p>
        <button className="button primary" onClick={() => void install()}><Plus size={17}/> Selecionar arquivo JSON</button>
      </div>}
    <details className="source-help glass-panel">
      <summary>Manifesto de extensão e documentação</summary>
      <div><h2>Formato de extensão v1</h2>
        <p>Uma fonte descreve um endpoint HTTPS com um adaptador JSON, OPDS ou HTML declarativo. Pesquisas e downloads em fila já funcionam quando a fonte oferece links diretos compatíveis. A importação é validada e pode ser automática quando autorizada.</p></div>
      <div className="source-help-actions">
        <button className="button secondary" onClick={() => setShowExample((current) => !current)}><FileJson2 size={16}/> {showExample ? "Ocultar exemplo" : "Ver exemplo"}</button>
        <button className="button secondary" onClick={() => void copyExample()}><ClipboardCopy size={16}/> Copiar JSON</button>
      </div>
      {showExample && <pre className="source-example">{sampleManifest}</pre>}
      <p className="source-disclaimer"><ExternalLink size={14}/> Instale apenas fontes de confiança e respeite as condições de distribuição dos arquivos.</p>
    </details>
    </>}
    {downloadChoice && <DownloadCatalogPicker book={downloadChoice} root={root} works={works} initial={initialSearch?.catalogTarget}
      busy={pending.has(resultKey(downloadChoice))} error={queueError} onClose={() => setDownloadChoice(null)}
      onSubmit={(target, autoImport, filenameSuffix) => void queue(downloadChoice, target, autoImport, filenameSuffix)}/>}
  </section>;
}
