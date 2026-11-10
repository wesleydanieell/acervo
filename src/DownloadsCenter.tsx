import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ArrowDownToLine, Check, CircleAlert, Download, FolderCheck, Pause, RefreshCw, RotateCw, Square, Trash2, X } from "lucide-react";
import "./downloads.css";

export type DownloadInfo = {
  id: string; title: string; sourceName: string; format: string; filename: string;
  plannedDestination: string; status: string; downloadedBytes: number;
  totalBytes: number | null; error: string; savedPath: string;
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
  const [confirmId, setConfirmId] = useState("");
  const [discardId, setDiscardId] = useState("");
  const [busyId, setBusyId] = useState("");

  const refresh = async () => {
    try {
      const result = await invoke<DownloadInfo[]>("list_downloads");
      setItems(result);
    } catch (failure) { setError(String(failure)); }
  };

  useEffect(() => {
    void refresh();
    const timer = window.setInterval(() => { void refresh(); }, 900);
    return () => window.clearInterval(timer);
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
    if (!root) { setError("Selecione uma biblioteca em Configurações antes de salvar."); return; }
    const destination = destinations[item.id] ?? (folders.includes(item.plannedDestination) ? item.plannedDestination : "");
    setBusyId(item.id); setError(""); setNotice("");
    try {
      const saved = await invoke<string>("save_download", { id: item.id, root, destination });
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
    <div className="downloads-note glass-panel"><ArrowDownToLine size={22}/>
      <div><strong>Primeiro em área temporária, depois no acervo.</strong>
        <p>Os arquivos ficam no cache até você confirmar o destino. A fila persiste entre reinícios; nada volta a baixar sozinho. Pause downloads sem perder os dados parciais. Use “Continuar” para tentar retomar de onde parou quando a fonte permitir (HTTP Range + ETag); sem esse suporte, a transferência recomeça do zero.</p></div>
    </div>
    {error && <div className="alert" role="alert"><CircleAlert size={17}/>{error}<button className="icon-button" onClick={() => setError("")} aria-label="Fechar erro"><X size={16}/></button></div>}
    {notice && <div className="inline-status" role="status"><Check size={16}/>{notice}</div>}
    {items.length === 0 ? <div className="empty-state glass-panel downloads-empty"><Download size={39}/><h3>Sua fila está vazia</h3>
      <p>Abra Fontes, pesquise um livro e use “Baixar” para adicioná-lo à fila.</p></div> :
      <div className="download-list">{items.map((item) => {
        const destination = destinations[item.id] ?? (folders.includes(item.plannedDestination) ? item.plannedDestination : "");
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
          {item.status === "saved" && <p className="download-saved"><FolderCheck size={15}/>{item.savedPath}</p>}
          {item.status === "ready" && <div className="download-destination">
            <label>Pasta de destino
              <select value={destination} onChange={(event) => { setDestinations((values) => ({ ...values, [item.id]: event.target.value })); setConfirmId(""); }}>
                <option value="">Raiz da biblioteca</option>
                {folders.filter((folder) => folder !== "99 - Catálogo" && !folder.startsWith("99 - Catálogo/")).map((folder) =>
                  <option key={folder} value={folder}>{folder}</option>)}
              </select>
            </label>
            {item.plannedDestination && !folders.includes(item.plannedDestination) && <small className="download-target-note">Pasta planejada “{item.plannedDestination}” não existe; selecione uma pasta válida.</small>}
            {discardId === item.id && <p className="download-confirm">Descartar definitivamente o arquivo temporário <strong>{item.filename}</strong>?</p>}
            {confirmId === item.id && <p className="download-confirm">Confirmar salvar <strong>{item.filename}</strong> em <strong>{destination || "Raiz"}</strong>? Nada será sobrescrito.</p>}
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
                  <button className="button primary" disabled={busyId === item.id || !root} onClick={() => void save(item)}><Check size={15}/> Confirmar</button></>
              : <button className="button primary" disabled={!root} onClick={() => setConfirmId(item.id)}><FolderCheck size={15}/> Salvar na biblioteca</button>)}
          </div>
        </article>;
      })}</div>}
  </section>;
}
