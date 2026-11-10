import { useEffect, useMemo, useState } from "react";
import { ArrowRight, Book, Check, ChevronDown, ClipboardCopy, FilePlus2, Filter, Search, Sparkles, Upload } from "lucide-react";
import "./catalog-center.css";
import { alternateEditions, indexEditions } from "./edition-matches";

export type CatalogItem = {
  id: number; title: string; author: string; publishers: string; destination: string;
  present: boolean; priority: boolean; associatedFiles: string[];
};

export type LocalBookFile = { path: string; title: string; folder: string; format: string };

type CatalogRow = CatalogItem & { fileOnly?: LocalBookFile };

type Props = {
  files: LocalBookFile[];
  onSelectFile: (file: LocalBookFile) => void;
  onCatalogUnindexed: (json: string) => void;
  works: CatalogItem[];
  query: string;
  onImport: () => void;
  onPrompt: () => void;
  onExport: (json: string) => void;
  onSelect: (work: CatalogItem) => void;
};

type Availability = "all" | "missing" | "available" | "unindexed" | "editions";
const fold = (value: string) => value.normalize("NFD").replace(/[\u0300-\u036f]/g, "").toLowerCase();

function publisherList(raw: string): string[] {
  if (!raw.trim()) return [];
  try {
    const parsed: unknown = JSON.parse(raw);
    if (Array.isArray(parsed)) return parsed.filter((value): value is string => typeof value === "string");
  } catch { /* Legacy catalogs may contain publisher names as plain text. */ }
  return [raw];
}

export function CatalogCenter({ works, files, query, onImport, onPrompt, onExport, onSelect, onSelectFile, onCatalogUnindexed }: Props) {
  const [availability, setAvailability] = useState<Availability>("all");
  const [destination, setDestination] = useState("all");
  const [limit, setLimit] = useState(80);
  const missing = works.filter((work) => !work.present).length;
  const editionIndex = useMemo(() => indexEditions(files), [files]);
  const editions = useMemo(() => new Map(works.map((work) => [work.id, alternateEditions(work, editionIndex)])), [works, editionIndex]);
  const alternateCount = works.filter((work) => (editions.get(work.id)?.length || 0) > 0).length;
  const unindexed = useMemo(() => {
    const linked = new Set(works.flatMap((work) => work.associatedFiles));
    return files.filter((file) => !linked.has(file.path));
  }, [works, files]);
  const allRows = useMemo((): CatalogRow[] => [
    ...works,
    ...unindexed.map((file, index): CatalogRow => ({
      id: -1000000 - index, title: file.title, author: "", publishers: "", destination: file.folder,
      present: true, priority: false, associatedFiles: [file.path], fileOnly: file,
    })),
  ], [works, unindexed]);
  const destinations = useMemo(() => [...new Set(allRows.map((work) => work.destination).filter(Boolean))].sort((a, b) => a.localeCompare(b, "pt-BR")), [allRows]);
  const filtered = useMemo(() => allRows.filter((work) => {
    if (availability === "unindexed" && !work.fileOnly) return false;
    if (availability === "editions" && !(editions.get(work.id)?.length || 0)) return false;
    if (availability === "missing" && work.present) return false;
    if (availability === "available" && !work.present) return false;
    if (destination !== "all" && work.destination !== destination) return false;
    return !query || fold([work.title, work.author, work.destination, work.publishers].join(" ")).includes(fold(query));
  }).sort((a, b) => Number(b.priority) - Number(a.priority) || a.title.localeCompare(b.title, "pt-BR")), [allRows, availability, destination, query, editions]);
  useEffect(() => setLimit(80), [availability, destination, query]);

  const catalogDiscoveredJson = () => JSON.stringify({
    versao: 1,
    obras: unindexed.map((file) => ({
      titulo: file.title, autor: "", local_indicado: file.folder,
      arquivos_encontrados: [file.path], prioridade_alta: 0,
    })),
  }, null, 2);

  const exportJson = () => JSON.stringify({
    versao: 1,
    obras: works.map((work) => ({
      titulo: work.title,
      autor: work.author,
      editoras_indicadas: publisherList(work.publishers),
      local_indicado: work.destination,
      prioridade_alta: work.priority ? 1 : 0,
      arquivos_encontrados: work.associatedFiles,
    })),
  }, null, 2);

  return <section className="catalog-center">
    <div className="page-intro">
      <div><div className="eyebrow">CENTRAL DE CATÁLOGO</div><h1>Catálogo de obras</h1>
        <p>Listas de livros, aquisições planejadas e obras faltantes. Separado dos arquivos reais da biblioteca.</p></div>
    </div>
    <div className="catalog-summary">
      <div className="catalog-summary-card glass-panel"><Book size={19}/><span>Obras cadastradas</span><strong>{allRows.length}</strong></div>
      <div className="catalog-summary-card glass-panel"><FilePlus2 size={19}/><span>Lista de desejos</span><strong>{missing}</strong></div>
      <div className="catalog-summary-card glass-panel"><Check size={19}/><span>Já disponíveis</span><strong>{allRows.length - missing}</strong></div>
    </div>
    <div className="catalog-actions">
      <button className="button primary" onClick={onImport}><Upload size={17}/> Importar JSON</button>
      <button className="button secondary" onClick={onPrompt}><Sparkles size={17}/> Gerar lista com IA</button>
      <button className="button secondary" disabled={!works.length} onClick={() => onExport(exportJson())}><ClipboardCopy size={17}/> Copiar JSON</button>
      {unindexed.length > 0 && <button className="button secondary" onClick={() => onCatalogUnindexed(catalogDiscoveredJson())}><Book size={17}/> Catalogar {unindexed.length} arquivo(s) sem ficha</button>}
    </div>
    <div className="catalog-toolbar">
      <div className="catalog-tabs" role="group" aria-label="Filtrar catálogo por disponibilidade">
        {([["all", "Todas", allRows.length], ["missing", "Faltantes", missing], ["available", "Disponíveis", allRows.length - missing], ["unindexed", "Sem ficha", unindexed.length], ["editions", "Outros formatos", alternateCount]] as const).map(([id, label, count]) =>
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
      {filtered.slice(0, limit).map((work) => <button className="catalog-work glass-panel" key={work.fileOnly?.path || work.id} onClick={() => work.fileOnly ? onSelectFile(work.fileOnly) : onSelect(work)}>
        <span className={"catalog-work-icon" + (work.present ? " is-available" : "")}>{work.present ? <Book size={20}/> : <FilePlus2 size={20}/>}</span>
        <span className="catalog-work-info"><strong>{work.title}</strong>
          <small>{work.author || "Autor não informado"}</small>
          <span className="catalog-work-destination">{work.destination || "Destino não definido"}</span></span>
        <span className="catalog-work-badges">{work.priority && <span className="catalog-priority">Prioridade</span>}
          {(editions.get(work.id)?.length || 0) > 0 && <span className="catalog-alternate" title="Mesmo título encontrado em outro arquivo; confirme a edição antes de associar">{work.present ? "Outro formato" : "Possível arquivo local"} · {editions.get(work.id)?.map((file) => file.format).join(", ")}</span>}
          <span className={work.fileOnly ? "catalog-unindexed" : work.present ? "catalog-available" : "catalog-missing"}>{work.fileOnly ? "Sem ficha" : work.present ? "Disponível" : "Faltante"}</span></span>
        <ArrowRight size={17} className="catalog-work-arrow"/>
      </button>)}
      {limit < filtered.length && <button className="load-more" onClick={() => setLimit((value) => value + 80)}>Mostrar mais <span>{limit} de {filtered.length}</span><ChevronDown size={16}/></button>}
    </div> : <div className="empty-state glass-panel"><Search size={34}/><h3>Nenhuma obra encontrada</h3><p>Altere os filtros ou importe uma lista JSON.</p></div>}
  </section>;
}
