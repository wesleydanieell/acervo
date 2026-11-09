import { useEffect, useMemo, useState } from "react";
import { ArrowRight, Book, Check, ChevronDown, ClipboardCopy, FilePlus2, Filter, Search, Sparkles, Upload } from "lucide-react";
import "./catalog-center.css";

export type CatalogItem = {
  id: number; title: string; author: string; publishers: string; destination: string;
  present: boolean; priority: boolean; associatedFiles: string[];
};

type Props = {
  works: CatalogItem[];
  query: string;
  onImport: () => void;
  onPrompt: () => void;
  onExport: (json: string) => void;
  onSelect: (work: CatalogItem) => void;
};

type Availability = "all" | "missing" | "available";
const fold = (value: string) => value.normalize("NFD").replace(/[\u0300-\u036f]/g, "").toLowerCase();

function publisherList(raw: string): string[] {
  if (!raw.trim()) return [];
  try {
    const parsed: unknown = JSON.parse(raw);
    if (Array.isArray(parsed)) return parsed.filter((value): value is string => typeof value === "string");
  } catch { /* Legacy catalogs may contain publisher names as plain text. */ }
  return [raw];
}

export function CatalogCenter({ works, query, onImport, onPrompt, onExport, onSelect }: Props) {
  const [availability, setAvailability] = useState<Availability>("all");
  const [destination, setDestination] = useState("all");
  const [limit, setLimit] = useState(80);
  const missing = works.filter((work) => !work.present).length;
  const destinations = useMemo(() => [...new Set(works.map((work) => work.destination).filter(Boolean))].sort((a, b) => a.localeCompare(b, "pt-BR")), [works]);
  const filtered = useMemo(() => works.filter((work) => {
    if (availability === "missing" && work.present) return false;
    if (availability === "available" && !work.present) return false;
    if (destination !== "all" && work.destination !== destination) return false;
    return !query || fold([work.title, work.author, work.destination, work.publishers].join(" ")).includes(fold(query));
  }).sort((a, b) => Number(b.priority) - Number(a.priority) || a.title.localeCompare(b.title, "pt-BR")), [works, availability, destination, query]);
  useEffect(() => setLimit(80), [availability, destination, query]);

  const exportJson = () => JSON.stringify({
    versao: 1,
    obras: works.map((work) => ({
      titulo: work.title,
      autor: work.author,
      editoras_indicadas: publisherList(work.publishers),
      local_indicado: work.destination,
      prioridade_alta: work.priority ? 1 : 0,
    })),
  }, null, 2);

  return <section className="catalog-center">
    <div className="page-intro">
      <div><div className="eyebrow">CENTRAL DE CATÁLOGO</div><h1>Catálogo de obras</h1>
        <p>Listas de livros, aquisições planejadas e obras faltantes. Separado dos arquivos reais da biblioteca.</p></div>
    </div>
    <div className="catalog-summary">
      <div className="catalog-summary-card glass-panel"><Book size={19}/><span>Obras cadastradas</span><strong>{works.length}</strong></div>
      <div className="catalog-summary-card glass-panel"><FilePlus2 size={19}/><span>Lista de desejos</span><strong>{missing}</strong></div>
      <div className="catalog-summary-card glass-panel"><Check size={19}/><span>Já disponíveis</span><strong>{works.length - missing}</strong></div>
    </div>
    <div className="catalog-actions">
      <button className="button primary" onClick={onImport}><Upload size={17}/> Importar JSON</button>
      <button className="button secondary" onClick={onPrompt}><Sparkles size={17}/> Gerar lista com IA</button>
      <button className="button secondary" disabled={!works.length} onClick={() => onExport(exportJson())}><ClipboardCopy size={17}/> Copiar JSON</button>
    </div>
    <div className="catalog-toolbar">
      <div className="catalog-tabs" role="group" aria-label="Filtrar catálogo por disponibilidade">
        {([["all", "Todas", works.length], ["missing", "Faltantes", missing], ["available", "Disponíveis", works.length - missing]] as const).map(([id, label, count]) =>
          <button key={id} className={availability === id ? "active" : ""} aria-pressed={availability === id}
            onClick={() => setAvailability(id)}>{label} <span>{count}</span></button>
        )}
      </div>
      <label className="catalog-destination"><Filter size={16}/><select aria-label="Filtrar pasta planejada" value={destination} onChange={(event) => setDestination(event.target.value)}>
        <option value="all">Todas as pastas planejadas</option>
        {destinations.map((folder) => <option key={folder} value={folder}>{folder}</option>)}
      </select><ChevronDown size={14}/></label>
    </div>
    <div className="catalog-list-heading"><strong>{filtered.length} obra(s)</strong><span>Prioridades aparecem primeiro</span></div>
    {filtered.length ? <div className="catalog-list">
      {filtered.slice(0, limit).map((work) => <button className="catalog-work glass-panel" key={work.id} onClick={() => onSelect(work)}>
        <span className={"catalog-work-icon" + (work.present ? " is-available" : "")}>{work.present ? <Book size={20}/> : <FilePlus2 size={20}/>}</span>
        <span className="catalog-work-info"><strong>{work.title}</strong>
          <small>{work.author || "Autor não informado"}</small>
          <span className="catalog-work-destination">{work.destination || "Destino não definido"}</span></span>
        <span className="catalog-work-badges">{work.priority && <span className="catalog-priority">Prioridade</span>}
          <span className={work.present ? "catalog-available" : "catalog-missing"}>{work.present ? "Disponível" : "Faltante"}</span></span>
        <ArrowRight size={17} className="catalog-work-arrow"/>
      </button>)}
      {limit < filtered.length && <button className="load-more" onClick={() => setLimit((value) => value + 80)}>Mostrar mais <span>{limit} de {filtered.length}</span><ChevronDown size={16}/></button>}
    </div> : <div className="empty-state glass-panel"><Search size={34}/><h3>Nenhuma obra encontrada</h3><p>Altere os filtros ou importe uma lista JSON.</p></div>}
  </section>;
}
