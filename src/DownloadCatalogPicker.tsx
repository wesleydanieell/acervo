import { useDeferredValue, useEffect, useMemo, useRef, useState } from "react";
import { Book, Check, Download, Folder, Search, X } from "lucide-react";
import type { CatalogItem } from "./CatalogCenter";
import { catalogChoice, sameCatalogChoice, filterDownloadWorks, validDownloadPath, type CatalogTarget } from "./download-workspace";

export function DownloadCatalogPicker({ book, root, works, initial, busy, error, onClose, onSubmit }: {
  book: { title: string; author: string; format: string; sourceName: string; language?:string; publisher?:string; editionLabel?:string };
  root: string; works: CatalogItem[]; initial?: CatalogTarget; busy: boolean; error: string;
  onClose: () => void; onSubmit: (target: CatalogTarget | null, autoImport: boolean) => void;
}) {
  const dialog = useRef<HTMLDialogElement>(null);
  const [query, setQuery] = useState("");
  const deferred = useDeferredValue(query);
  const [selected, setSelected] = useState<CatalogTarget | null>(() => initial ?? null);
  const [automatic, setAutomatic] = useState(true);
  const [limit, setLimit] = useState(80);
  const visible = useMemo(() => filterDownloadWorks(works, deferred), [works, deferred]);
  const matches = selected && root ? works.filter(w => sameCatalogChoice(catalogChoice(root, w), selected)) : [];
  const work = matches.length === 1 ? matches[0] : undefined;
  const target = work && root ? catalogChoice(root, work) : null;
  const destination = target?.destination === "." ? "" : target?.destination;
  const autoAllowed = !!target && (destination === "" || validDownloadPath(destination || ""));
  useEffect(() => { dialog.current?.showModal(); }, []);
  return <dialog ref={dialog} className="download-picker glass-panel" aria-labelledby="download-picker-title"
    onCancel={e => { e.preventDefault(); if (!busy) onClose(); }}>
    <div className="download-picker-header"><div><h2 id="download-picker-title">Baixar livro</h2><p>{[book.title,book.editionLabel,book.format.toUpperCase(),book.language,book.publisher,book.sourceName].filter(Boolean).join(" · ")}</p></div>
      <button className="icon-button" aria-label="Fechar" disabled={busy} onClick={onClose}><X size={19}/></button></div>
    <div className="download-picker-body">
      <label className="download-picker-search"><Search size={16}/><input autoFocus value={query} aria-label="Pesquisar obra no catálogo" placeholder="Título, autor ou pasta…" onChange={e => { setQuery(e.target.value); setLimit(80); }}/></label>
      <div className="download-picker-list" role="group" aria-label="Obra para associar">
        <button className="download-picker-row" aria-pressed={selected === null} disabled={busy} onClick={() => setSelected(null)}><Download size={17}/><span><strong>Sem associação</strong><small>Baixar para a fila e escolher o destino depois</small></span>{selected === null && <Check size={17}/>}</button>
        {root && visible.slice(0, limit).map(w => <button key={w.id} className="download-picker-row" aria-pressed={work?.id === w.id} disabled={busy} onClick={() => setSelected(catalogChoice(root, w))}>
          <Book size={17}/><span><strong>{w.title}</strong><small>{w.author || "Sem autor"} · {w.destination || "Raiz"} · {w.present ? "Disponível" : "Faltante"}</small></span>{work?.id === w.id && <Check size={17}/>}
        </button>)}
        {!root ? <p className="download-target-note">Selecione uma biblioteca para vincular obras.</p> : !visible.length ? <p className="download-target-note">Nenhuma obra encontrada.</p> : null}
        {visible.length > limit && <button className="button secondary" onClick={() => setLimit(n => n + 80)}>Mostrar mais ({limit} de {visible.length})</button>}
      </div>
      {work && target && <div className="download-picker-details"><strong>{work.title}</strong><small>{work.author || "Sem autor"}{work.publishers ? " · " + work.publishers : ""}</small><div><Folder size={16}/> <span>{destination || "Raiz da biblioteca"}</span></div>
        <label><input type="checkbox" checked={automatic && autoAllowed} disabled={!autoAllowed || busy} onChange={e => setAutomatic(e.target.checked)}/> Importar e atualizar catálogo ao concluir</label>
        {!autoAllowed && <small>Destino inválido; escolha a pasta na fila após baixar.</small>}
      </div>}
      {selected && !target && <p className="download-error" role="status">A ficha está indisponível ou duplicada. Escolha outra obra.</p>}
      {error && <p className="download-error" role="alert">{error}</p>}
    </div>
    <div className="download-picker-footer"><button className="button secondary" disabled={busy} onClick={onClose}>Cancelar</button><button className="button primary" disabled={busy || (selected !== null && !target)} onClick={() => onSubmit(target, automatic && autoAllowed)}><Download size={16}/>{busy ? "Adicionando…" : target && automatic && autoAllowed ? "Baixar e importar" : "Baixar"}</button></div>
  </dialog>;
}
