import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Check, CircleAlert, Download, FolderCheck, FolderPlus, Pause, RefreshCw, RotateCw, Square, Trash2, X } from "lucide-react";
import "./downloads.css";
import { planDownloadDestination, validDownloadPath, downloadGroup, canImportDownload, type CatalogTarget, type DownloadFilter } from "./download-workspace";
import { isServiceCatalogPath } from "./prompt-folder-selection";

export type DownloadInfo = {
  id: string; title: string; sourceName: string; format: string; filename: string;
  plannedDestination: string; status: string; downloadedBytes: number;
  totalBytes: number | null; error: string; savedPath: string; catalogTarget?: CatalogTarget | null; catalogLinked?: boolean;
};

const sizes = (bytes: number) => bytes >= 1048576
  ? (bytes / 1048576).toFixed(1) + " MB"
  : (bytes / 1024).toFixed(1) + " KB";
const statuses: Record<string, string> = {
  queued: "Na fila", downloading: "Baixando", pausing: "Pausando", paused: "Pausado", ready: "Pronto para salvar",
  failed: "Falhou", cancelled: "Cancelado", saving: "Salvando", saved: "Na biblioteca",
};

export function DownloadsCenter({ root, folders, onSaved }: {
  root: string; folders: string[]; onSaved: () => Promise<void>;
}) {
  const [items, setItems] = useState<DownloadInfo[]>([]);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [destinations, setDestinations] = useState<Record<string, string>>({});
  const [customPaths, setCustomPaths] = useState<Record<string, string>>({});
  const [confirmId, setConfirmId] = useState("");
  const [linkChoices, setLinkChoices] = useState<Record<string, boolean>>({});
  const [discardId, setDiscardId] = useState("");
  const [busyId, setBusyId] = useState("");
  const [filter, setFilter] = useState<DownloadFilter>("all");
  const [query, setQuery] = useState("");
  const refreshGeneration = useRef(0);
  const allowedFolders = useMemo(() => folders.filter(folder => !isServiceCatalogPath(folder)), [folders]);
  const counts = useMemo(() => {
    const result = { all: items.length, active: 0, ready: 0, history: 0 };
    for (const item of items) result[downloadGroup(item.status)]++;
    return result;
  }, [items]);
  const visible = useMemo(() => {
    const needle = query.trim().toLocaleLowerCase("pt-BR");
    return items.filter(item => (filter === "all" || downloadGroup(item.status) === filter)
      && (!needle || [item.title, item.sourceName, item.filename].some(value => value.toLocaleLowerCase("pt-BR").includes(needle))));
  }, [items, filter, query]);
  useEffect(() => { setDestinations({}); setCustomPaths({}); setLinkChoices({}); setConfirmId(""); setDiscardId(""); }, [root]);

  const refresh = async () => {
    const generation = ++refreshGeneration.current;
    try {
      const result = await invoke<DownloadInfo[]>("list_downloads");
      if (generation === refreshGeneration.current) setItems(previous => JSON.stringify(previous) === JSON.stringify(result) ? previous : result);
    } catch (failure) { if (generation === refreshGeneration.current) setError(String(failure)); }
  };

  useEffect(() => {
    void refresh();
    const timer = window.setInterval(() => { void refresh(); }, 900);
    return () => { window.clearInterval(timer); refreshGeneration.current++; };
  }, []);

  const action = async (command: string, id: string) => {
    setBusyId(id); setError(""); setNotice("");
    try {
      await invoke(command, { id });
      await refresh();
    } catch (failure) { setError(String(failure)); }
    finally { setBusyId(""); }
  };

  const save = async (item: DownloadInfo) => {
    if (!canImportDownload(root, item.catalogTarget)) { setError("Selecione a biblioteca de origem antes de salvar."); return; }
    const plan = planDownloadDestination(item.plannedDestination, destinations[item.id], customPaths[item.id], allowedFolders);
    if (plan === null) { setError("Escolha uma pasta de destino válida."); return; }
    setBusyId(item.id); setError(""); setNotice("");
    try {
      const saved = await invoke<string>("save_download", { id: item.id, root, destination: plan.path, createMissing: plan.createMissing, linkCatalog: Boolean(item.catalogTarget && (linkChoices[item.id] ?? true)) });
      setConfirmId("");
      setNotice("Salvo em " + saved);
      await onSaved();
      await refresh();
    } catch (failure) { setError(String(failure)); }
    finally { setBusyId(""); }
  };

  return <section className="downloads-center">
    <div className="page-intro"><div><div className="eyebrow">GERENCIADOR DE DOWNLOADS</div>
      <h1>Downloads</h1><p>Acompanhe transferências e decida onde cada livro será salvo na biblioteca.</p></div>
      <button className="button secondary" onClick={() => void refresh()}><RefreshCw size={17}/> Atualizar</button>
    </div>
    <div className="download-workspace-toolbar">
      <div className="download-filters" role="group" aria-label="Filtrar downloads">
        {([["all", "Todos"], ["active", "Transferências"], ["ready", "A importar"], ["history", "Histórico"]] as const).map(([value, label]) =>
          <button key={value} type="button" aria-pressed={filter === value} onClick={() => setFilter(value)}>{label}<span>{counts[value]}</span></button>)}
      </div>
      <input className="download-search" aria-label="Pesquisar downloads" placeholder="Pesquisar downloads" value={query} onChange={event => setQuery(event.target.value)}/>
    </div>
    {error && <div className="alert" role="alert"><CircleAlert size={17}/>{error}<button className="icon-button" onClick={() => setError("")} aria-label="Fechar erro"><X size={16}/></button></div>}
    {notice && <div className="inline-status" role="status"><Check size={16}/>{notice}</div>}
    {items.length === 0 ? <div className="empty-state glass-panel downloads-empty"><Download size={39}/><h3>Sua fila está vazia</h3>
      <p>Abra Descobrir, pesquise um livro e use “Baixar” para adicioná-lo à fila.</p></div> :
      visible.length === 0 ? <div className="empty-state glass-panel downloads-empty"><Download size={28}/><h3>Nenhum download neste filtro</h3><button className="button secondary" onClick={() => { setQuery(""); setFilter("all"); }}>Limpar filtros</button></div> :
      <div className="download-list">{visible.map((item) => {
        const custom = customPaths[item.id];
        const plan = planDownloadDestination(item.plannedDestination, destinations[item.id], custom, allowedFolders);
        const destination = plan?.path ?? null;
        const canImport = canImportDownload(root, item.catalogTarget);
        const willLink = Boolean(item.catalogTarget && (linkChoices[item.id] ?? true));
        const percentage = item.totalBytes ? Math.min(100, 100 * item.downloadedBytes / item.totalBytes) : undefined;
        return <article key={item.id} className="download-card glass-panel">
          <div className="download-card-top"><div className="download-icon"><Download size={21}/></div>
            <div className="download-title"><strong title={item.title}>{item.title}</strong><small>{item.sourceName} · {item.format.toUpperCase()} · {item.filename}</small></div>
            <span className={"download-status status-" + item.status}>{statuses[item.status] || item.status}</span>
          </div>
          {(["queued", "downloading", "pausing", "paused"].includes(item.status)) && <div className="download-progress">
            <progress max={100} value={percentage ?? undefined} aria-label={"Progresso de " + item.title}/>
            <span>{sizes(item.downloadedBytes)}{item.totalBytes ? " de " + sizes(item.totalBytes) + " · " + percentage?.toFixed(0) + "%" : " · tamanho desconhecido"}</span></div>}
          {item.error && <p className="download-error" role="status">{item.error}</p>}
          {item.status === "failed" && item.downloadedBytes > 0 &&
            <p className="download-target-note">Arquivo parcial em cache: {sizes(item.downloadedBytes)}. A próxima tentativa verifica se a fonte permite retomá-lo.</p>}
          {item.status === "saved" && <p className="download-saved"><FolderCheck size={15}/>{item.savedPath}{item.catalogLinked ? " · Associado ao catálogo" : ""}</p>}
          {item.catalogTarget && <small className="download-target-note">Obra: {item.catalogTarget.title}{item.catalogTarget.author ? " — " + item.catalogTarget.author : ""}</small>}
          {item.status === "ready" && item.catalogTarget && !canImport && <p className="download-error">Biblioteca de origem: {item.catalogTarget.root}. Selecione essa biblioteca para importar.</p>}
          {item.status === "ready" && <div className="download-destination">
            {custom === undefined ? <label>Pasta de destino
              <select value={destination ?? "__choose__"} onChange={(event) => { setDestinations((values) => ({ ...values, [item.id]: event.target.value })); setConfirmId(""); }}>
                {destination === null && <option value="__choose__" disabled>Escolha uma pasta</option>}
                <option value="">Raiz da biblioteca</option>
                {allowedFolders.map((folder) =>
                  <option key={folder} value={folder}>{folder}</option>)}
              </select>
            </label> : <label>Caminho relativo à biblioteca
              <input className="download-custom-path" value={custom} maxLength={600}
                placeholder="Literatura/Clássicos" aria-invalid={custom.length > 0 && !validDownloadPath(custom)}
                onChange={event => { setCustomPaths(values => ({ ...values, [item.id]: event.target.value })); setConfirmId(""); }}/>
            </label>}
            <div className="download-path-actions">
              {custom === undefined ? <button className="button secondary" disabled={!root} onClick={() => {
                setCustomPaths(values => ({ ...values, [item.id]: validDownloadPath(item.plannedDestination) ? item.plannedDestination : "" })); setConfirmId("");
              }}><FolderPlus size={15}/> Caminho personalizado</button> :
              <button className="button secondary" onClick={() => { setCustomPaths(values => { const next = { ...values }; delete next[item.id]; return next; }); setConfirmId(""); }}>Escolher pasta existente</button>}
            </div>
            {custom === undefined && item.plannedDestination && !allowedFolders.includes(item.plannedDestination) && <small className="download-target-note">Pasta planejada: {item.plannedDestination}. Escolha uma pasta existente ou crie o caminho planejado.</small>}
            {custom !== undefined && custom.length > 0 && !validDownloadPath(custom) && <small className="download-target-note" role="status">Caminho inválido. Use nomes de pastas separados por /.</small>}
            {discardId === item.id && <p className="download-confirm">Descartar definitivamente o arquivo temporário <strong>{item.filename}</strong>?</p>}
            {item.catalogTarget && <label className="download-link-option"><input type="checkbox" checked={willLink} onChange={event => { setLinkChoices(values => ({ ...values, [item.id]: event.target.checked })); setConfirmId(""); }}/> Associar à obra do catálogo</label>}
            {confirmId === item.id && plan && <p className="download-confirm">Salvar <strong>{item.filename}</strong> em <strong>{root}{destination ? "/" + destination : ""}</strong>? {plan.createMissing ? "Pastas inexistentes serão criadas. " : ""}{willLink ? "O arquivo será associado à obra selecionada. " : ""}Nada será sobrescrito.</p>}
          </div>}
          <div className="download-actions">
            {(item.status === "queued" || item.status === "downloading") &&
              <button className="button secondary" disabled={busyId === item.id} onClick={() => void action("pause_download", item.id)}><Pause size={15}/> Pausar</button>}
            {["queued", "downloading", "pausing", "paused"].includes(item.status) &&
              <button className="button secondary" disabled={busyId === item.id} onClick={() => void action("cancel_download", item.id)}><Square size={14}/> Cancelar</button>}
            {["failed", "cancelled", "paused"].includes(item.status) &&
              <button className="button secondary" disabled={busyId === item.id} onClick={() => void action("retry_download", item.id)}><RotateCw size={15}/> {item.status === "paused" ? "Continuar" : "Tentar novamente"}</button>}
            {item.status === "ready" && (discardId === item.id
              ? <><button className="button secondary" onClick={() => setDiscardId("")}>Voltar</button>
                  <button className="button danger" onClick={() => { setDiscardId(""); void action("dismiss_download", item.id); }}>Descartar download</button></>
              : <button className="button secondary" onClick={() => { setConfirmId(""); setDiscardId(item.id); }}><Trash2 size={15}/> Descartar</button>)}
            {["saved", "failed", "cancelled"].includes(item.status) &&
              <button className="button secondary" onClick={() => void action("dismiss_download", item.id)}><Trash2 size={15}/> Remover da lista</button>}
            {item.status === "ready" && discardId !== item.id && (confirmId === item.id
              ? <><button className="button secondary" onClick={() => setConfirmId("")}>Voltar</button>
                  <button className="button primary" disabled={busyId === item.id || !canImport || destination === null} onClick={() => void save(item)}><Check size={15}/> Confirmar</button></>
              : <button className="button primary" disabled={!canImport || destination === null} onClick={() => setConfirmId(item.id)}><FolderCheck size={15}/> Salvar na biblioteca</button>)}
          </div>
        </article>;
      })}</div>}
  </section>;
}
