import { Fragment, useEffect, useMemo, useRef, useState } from "react";
import {
  Archive, ArrowRight, ArrowUpDown, Book, Check, ChevronDown, ChevronRight,
  CircleHelp, ClipboardCopy, FilePlus2, Filter, Folder, FolderOpen,
  FolderPlus, FolderTree, Grid2X2, HardDrive, Home, Layers3, List, LoaderCircle, Menu,
  Palette, Plus, RefreshCw, Search, Settings2, Sparkles,
  Upload, X,
} from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import "./App.css";
import "./theme.css";
import "./explorer.css";
import "./catalog.css";

type BookFile = { id: string; title: string; path: string; folder: string; format: string; bytes: number };
type CatalogWork = { id: number; title: string; author: string; publishers: string; destination: string; present: boolean; priority: boolean; associatedFiles: string[] };
type Collection = { root: string; files: BookFile[]; folders: string[]; works: CatalogWork[]; warnings: string[] };
type View = "catalog" | "settings";
type Availability = "all" | "available" | "missing";
type GroupBy = "none" | "folder" | "format" | "author";
type Layout = "grid" | "list";
type Entry = { key: string; type: "file" | "work"; title: string; author: string; format: string; folder: string; present: boolean; file?: BookFile; work?: CatalogWork };
type PaletteName = "noctalia" | "jade" | "indigo" | "rose" | "amber";
type VisualStyle = "glass" | "material";
type SortBy = "title" | "author" | "format" | "size";
type ThemeTokens = Record<"primary" | "on_primary" | "surface" | "on_surface" | "surface_container" | "outline_variant", string> & { mode?: "light" | "dark" };

const accentPresets: Record<Exclude<PaletteName, "noctalia">, string> = {
  jade: "#8ddbb8", indigo: "#a6b7ff", rose: "#f8a9c7", amber: "#f2c27c",
};
const formatOptions = ["Todos", "EPUB", "PDF", "MOBI", "AZW3", "DJVU"];
const languageOptions = [{ label: "Todos os idiomas", value: "" }, { label: "Português", value: "pt" }, { label: "Inglês", value: "en" }, { label: "Espanhol", value: "es" }];
const fold = (value: string) => value.normalize("NFD").replace(/[\u0300-\u036f]/g, "").toLowerCase();
const readableBytes = (bytes: number) => bytes > 1024 * 1024 ? (bytes / 1024 ** 2).toFixed(1) + " MB" : Math.round(bytes / 1024) + " KB";

function getEntries(collection: Collection): Entry[] {
  const byPath = new Map(collection.files.map((file) => [file.path, file]));
  const assigned = new Set(collection.works.flatMap((work) => work.associatedFiles));
  return [
    ...collection.works.map((work): Entry => {
      const linked = work.associatedFiles.map((path) => byPath.get(path)).find(Boolean);
      return { key: "work:" + work.id, type: "work", title: work.title, author: work.author, format: linked?.format || "PENDENTE", folder: work.destination || ".", present: Boolean(linked), file: linked, work };
    }),
    ...collection.files.filter((file) => !assigned.has(file.path)).map((file): Entry => ({
      key: file.id + ":" + file.path, type: "file", title: file.title, author: "", format: file.format, folder: file.folder, present: true, file,
    })),
  ].sort((a, b) => a.title.localeCompare(b.title, "pt-BR"));
}

function App() {
  const [root, setRoot] = useState(() => localStorage.getItem("acervo-root") || "");
  const [collection, setCollection] = useState<Collection | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");
  const [view, setView] = useState<View>("catalog");
  const [availability, setAvailability] = useState<Availability>("all");
  const [groupBy, setGroupBy] = useState<GroupBy>("none");
  const [sidebarWidth, setSidebarWidth] = useState(() => Math.min(450, Math.max(210, Number(localStorage.getItem("acervo-sidebar-width")) || 254)));
  const sidebarResize = useRef<{ x: number; width: number } | null>(null);
  const [searchFocused, setSearchFocused] = useState(false);
  const [folderQuery, setFolderQuery] = useState("");
  const [activeSuggestion, setActiveSuggestion] = useState(-1);
  const [layout, setLayout] = useState<Layout>("grid");
  const [sortBy, setSortBy] = useState<SortBy>("title");
  const [selectedFolder, setSelectedFolder] = useState("");
  const [search, setSearch] = useState("");
  const [format, setFormat] = useState("Todos");
  const [selection, setSelection] = useState<Entry | null>(null);
  const [modal, setModal] = useState<"root" | "prompt" | "import" | "folder" | "move" | null>(null);
  const [sideOpen, setSideOpen] = useState(false);
  const [palette, setPalette] = useState<PaletteName>(() => (localStorage.getItem("acervo-palette") as PaletteName) || "noctalia");
  const [visualStyle, setVisualStyle] = useState<VisualStyle>(() => localStorage.getItem("acervo-visual-style") === "material" ? "material" : "glass");
  const [theme, setTheme] = useState<ThemeTokens | null>(null);
  const searchRef = useRef<HTMLInputElement>(null);
  const [importText, setImportText] = useState("");
  const [importNotice, setImportNotice] = useState("");
  const [promptCount, setPromptCount] = useState(10);
  const [promptTopics, setPromptTopics] = useState("Filosofia, literatura clássica e brasileira");
  const [promptLanguage, setPromptLanguage] = useState("Português");
  const [sourceFormat, setSourceFormat] = useState("");
  const [sourceLanguage, setSourceLanguage] = useState("pt");
  const [newFolder, setNewFolder] = useState("");
  const [folderParent, setFolderParent] = useState("");
  const [status, setStatus] = useState("");
  const [dragTarget, setDragTarget] = useState("");
  const [expandedFolders, setExpandedFolders] = useState<Set<string>>(() => new Set());
  const [selectionMode, setSelectionMode] = useState(false);
  const [bulkSelected, setBulkSelected] = useState<Set<string>>(() => new Set());
  const [pendingMove, setPendingMove] = useState<{ paths: string[]; destination: string } | null>(null);
  const [visibleLimit, setVisibleLimit] = useState(80);
  useEffect(() => { localStorage.setItem("acervo-sidebar-width", String(sidebarWidth)); }, [sidebarWidth]);

  const refresh = async (location = root) => {
    if (!location.trim()) { setModal("root"); return; }
    setLoading(true); setError("");
    try {
      const data = await invoke<Collection>("load_collection", { root: location.trim() });
      setCollection(data); setRoot(data.root); localStorage.setItem("acervo-root", data.root);
      setStatus(data.files.length + " arquivos encontrados; nenhuma alteração feita nos livros.");
      setModal(null);
    } catch (failure) { setError(String(failure)); }
    finally { setLoading(false); }
  };

  const chooseRoot = async () => {
    try {
      const chosen = await open({ directory: true, multiple: false, title: "Selecionar a pasta raiz do Acervo", defaultPath: collection?.root || undefined });
      if (typeof chosen === "string") await refresh(chosen);
    } catch (failure) { setError("Não foi possível abrir o explorador: " + String(failure)); }
  };

  useEffect(() => { if (root) void refresh(root); }, []); // Load the last library once.
  useEffect(() => {
    const onKeyboard = (event: KeyboardEvent) => {
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "k") {
        event.preventDefault(); setView("catalog"); setSelectedFolder(""); searchRef.current?.focus();
      }
      if (event.key === "Escape") { setSelection(null); setModal(null); }
    };
    window.addEventListener("keydown", onKeyboard);
    return () => window.removeEventListener("keydown", onKeyboard);
  }, []);
  useEffect(() => {
    let cancelled = false;
    const read = async () => {
      try {
        const tokens = await invoke<ThemeTokens | null>("read_noctalia_colors");
        if (!cancelled) setTheme(tokens);
      } catch { if (!cancelled) setTheme(null); }
    };
    void read(); const timer = window.setInterval(read, 4000);
    return () => { cancelled = true; window.clearInterval(timer); };
  }, []);
  useEffect(() => {
    localStorage.setItem("acervo-visual-style", visualStyle);
    document.documentElement.dataset.visualStyle = visualStyle;
  }, [visualStyle]);
  useEffect(() => {
    localStorage.setItem("acervo-palette", palette);
    const element = document.documentElement;
    element.dataset.colorMode = palette === "noctalia" && theme?.mode === "light" ? "light" : "dark";
    const main = palette === "noctalia" && theme ? theme.primary : accentPresets[palette === "noctalia" ? "jade" : palette];
    element.style.setProperty("--accent", main);
    element.style.setProperty("--accent-ink", palette === "noctalia" && theme ? theme.on_primary : "#142b2c");
    if (palette === "noctalia" && theme) {
      element.style.setProperty("--glass-base", theme.surface);
      element.style.setProperty("--text", theme.on_surface);
      element.style.setProperty("--glass-strong", theme.surface_container);
      element.style.setProperty("--outline", theme.outline_variant);
    } else {
      for (const key of ["--glass-base", "--text", "--glass-strong", "--outline"]) element.style.removeProperty(key);
    }
  }, [palette, theme]);

  const entries = useMemo(() => collection ? getEntries(collection) : [], [collection]);
  const filtered = useMemo(() => entries.filter((entry) => {
    if (availability === "available" && !entry.present) return false;
    if (availability === "missing" && entry.present) return false;
    if (selectedFolder && !(entry.folder === selectedFolder || entry.folder.startsWith(selectedFolder + "/"))) return false;
    if (format !== "Todos" && entry.format !== format) return false;
    if (search && !fold(entry.title + " " + entry.author + " " + entry.folder).includes(fold(search))) return false;
    return true;
  }).sort((a, b) => {
    if (sortBy === "size") return (b.file?.bytes || 0) - (a.file?.bytes || 0) || a.title.localeCompare(b.title, "pt-BR");
    const left = sortBy === "author" ? a.author : sortBy === "format" ? a.format : a.title;
    const right = sortBy === "author" ? b.author : sortBy === "format" ? b.format : b.title;
    return left.localeCompare(right, "pt-BR") || a.title.localeCompare(b.title, "pt-BR");
  }), [entries, availability, selectedFolder, format, search, sortBy]);
  const breadcrumbs = selectedFolder.split("/").filter(Boolean).map((label, index, parts) => ({ label, path: parts.slice(0, index + 1).join("/") }));
  useEffect(() => setVisibleLimit(80), [availability, selectedFolder, format, search, sortBy, groupBy]);
  const groupLabel = (entry: Entry) => groupBy === "folder" ? entry.folder || "Raiz" : groupBy === "format" ? entry.format : groupBy === "author" ? entry.author || "Sem autor" : "";
  const displayEntries = useMemo(() => groupBy === "none" ? filtered : [...filtered].sort((a, b) =>
    groupLabel(a).localeCompare(groupLabel(b), "pt-BR") || a.title.localeCompare(b.title, "pt-BR")
  ), [filtered, groupBy]);
  const visibleEntries = displayEntries.slice(0, visibleLimit);
  const suggestions = useMemo(() => {
    const needle = fold(search.trim());
    if (!needle || !searchFocused) return [] as { type: "Livro" | "Autor" | "Pasta"; value: string }[];
    const result: { type: "Livro" | "Autor" | "Pasta"; value: string }[] = [];
    const seen = new Set<string>();
    const add = (type: "Livro" | "Autor" | "Pasta", value: string) => {
      const key = type + ":" + fold(value);
      if (value && fold(value).includes(needle) && !seen.has(key)) { seen.add(key); result.push({ type, value }); }
    };
    for (const entry of entries) { add("Livro", entry.title); if (result.length >= 4) break; }
    for (const entry of entries) { add("Autor", entry.author); if (result.length >= 6) break; }
    for (const folder of collection?.folders || []) { add("Pasta", folder); if (result.length >= 8) break; }
    return result.slice(0, 8);
  }, [search, searchFocused, entries, collection]);
  const chooseSuggestion = (suggestion: { type: "Livro" | "Autor" | "Pasta"; value: string }) => {
    if (suggestion.type === "Pasta") { setSelectedFolder(suggestion.value); setSearch(""); }
    else setSearch(suggestion.value);
    setView("catalog"); setSearchFocused(false); setActiveSuggestion(-1);
  };
  const folderParents = useMemo(() => new Set(collection?.folders.map((folder) => folder.split("/").slice(0, -1).join("/")) || []), [collection]);
  const visibleFolders = useMemo(() => (collection?.folders || []).filter((folder) => {
    if (folderQuery.trim()) return fold(folder).includes(fold(folderQuery.trim()));
    const parts = folder.split("/");
    return parts.slice(0, -1).every((_, index) => expandedFolders.has(parts.slice(0, index + 1).join("/")));
  }), [collection, expandedFolders, folderQuery]);
  const subfolders = useMemo(() => (collection?.folders || []).filter((folder) =>
    folder.split("/").slice(0, -1).join("/") === selectedFolder
  ), [collection, selectedFolder]);
  const folderFileCounts = useMemo(() => {
    const counts = new Map<string, number>();
    for (const file of collection?.files || []) {
      const parts = file.folder.split("/").filter(Boolean);
      for (let index = 0; index < parts.length; index++) {
        const folder = parts.slice(0, index + 1).join("/");
        counts.set(folder, (counts.get(folder) || 0) + 1);
      }
    }
    return counts;
  }, [collection]);
  const moveConflicts = useMemo(() => {
    if (!pendingMove || !collection) return [];
    const existing = new Set(collection.files.map((book) => book.path));
    const targets = new Set<string>();
    return pendingMove.paths.filter((path) => {
      const name = path.split("/").slice(-1)[0];
      const target = [pendingMove.destination, name].filter(Boolean).join("/");
      if (path === target || existing.has(target) || targets.has(target)) return true;
      targets.add(target);
      return false;
    });
  }, [pendingMove, collection]);
  const missing = entries.filter((entry) => !entry.present).length;
  const currentLabel = selectedFolder ? selectedFolder.split("/").slice(-1)[0] : "Minha biblioteca";

  const createPrompt = () => {
    const folders = collection?.folders.length ? collection.folders.map((folder) => "- " + folder).join("\n") : "- . (raiz)";
    const sample = { versao: 1, obras: [{ titulo: "O Banquete", autor: "Platão", editoras_indicadas: ["Editora de exemplo"], local_indicado: "Literatura/Filosofia", enfoques: "Filosofia", categorias: "Clássicos", prioridade_alta: 0 }] };
    return `Gere uma lista de ${promptCount} livros reais em JSON para importar no meu gerenciador de acervo.

ASSUNTOS: ${promptTopics}
IDIOMA PREFERIDO: ${promptLanguage}
CRITÉRIOS: sugira obras e editoras verificáveis, não invente edições; evite títulos repetidos.
PASTAS EXISTENTES (use uma destas em local_indicado):
${folders}
Já possuo ${collection?.files.length || 0} arquivos indexados e ${collection?.works.length || 0} obras cadastradas.

RESPONDA APENAS JSON VÁLIDO. Cada obra tem titulo (obrigatório), autor, editoras_indicadas (lista de strings), local_indicado (caminho RELATIVO, sem ../), enfoques, categorias e prioridade_alta (0/1). Não inclua URLs ou arquivos_encontrados se não forem conhecidos. Exemplo:
${JSON.stringify(sample, null, 2)}`;
  };

  const copy = async (text: string, message: string) => {
    try { await navigator.clipboard.writeText(text); setStatus(message); }
    catch { setStatus("Não foi possível copiar; selecione e copie manualmente."); }
  };
  const searchAnna = (entry: Entry) => {
    const url = new URL("https://annas-archive.pk/search");
    url.searchParams.set("q", [entry.title, entry.author].filter(Boolean).join(" "));
    if (sourceFormat) url.searchParams.set("ext", sourceFormat.toLowerCase());
    if (sourceLanguage) url.searchParams.set("lang", sourceLanguage);
    void openUrl(url.toString()).catch((failure) => setError(String(failure)));
  };
  const queueMove = (paths: string[], destination: string) => {
    if (!collection || !paths.length) return;
    const unique = [...new Set(paths)];
    if (unique.length > 100) { setError("Selecione no máximo 100 livros por movimentação."); return; }
    setPendingMove({ paths: unique, destination });
    setDragTarget("");
    setModal("move");
  };
  const commitMove = async () => {
    if (!collection || !pendingMove) return;
    setLoading(true); setError("");
    try {
      const moved = await invoke<string[]>("move_books", {
        root: collection.root, relativePaths: pendingMove.paths, destination: pendingMove.destination,
      });
      await refresh(collection.root);
      setBulkSelected(new Set()); setSelectionMode(false); setSelection(null);
      setPendingMove(null); setModal(null); setStatus(moved.length + " livro(s) movido(s). Catálogo atualizado.");
    } catch (failure) { setError(String(failure)); }
    finally { setLoading(false); }
  };
  const toggleBookSelection = (path: string) => {
    setSelectionMode(true);
    setBulkSelected((current) => {
      const next = new Set(current);
      if (next.has(path)) next.delete(path); else next.add(path);
      return next;
    });
  };
  const dropPaths = (data: DataTransfer): string[] => {
    try {
      const many = JSON.parse(data.getData("text/acervo-books"));
      if (Array.isArray(many) && many.every((item) => typeof item === "string")) return many;
    } catch { /* A normal single-file drag has no JSON payload. */ }
    const single = data.getData("text/acervo-book");
    return single ? [single] : [];
  };
  const addFolder = async () => {
    if (!collection || !newFolder.trim()) return;
    try {
      const created = await invoke<string>("create_folder", { root: collection.root, parent: folderParent, name: newFolder.trim() });
      await refresh(collection.root); setSelectedFolder(created); setView("catalog"); setNewFolder(""); setStatus("Pasta criada: " + created);
      setExpandedFolders((current) => {
        const next = new Set(current);
        const parts = created.split("/");
        for (let index = 1; index < parts.length; index++) next.add(parts.slice(0, index).join("/"));
        return next;
      });
    } catch (failure) { setError(String(failure)); }
  };

  return (
    <div className="app-shell">
      <div className="background-orb orb-a" /><div className="background-orb orb-b" />
      <aside className={"sidebar glass-panel" + (sideOpen ? " open" : "")} style={{ width: sidebarWidth }}>
        <div className="brand"><span className="brand-symbol"><Archive size={22} /></span><div><strong>Acervo</strong><small>Organização pessoal</small></div><button className="icon-button mobile-close" onClick={() => setSideOpen(false)} aria-label="Fechar menu"><X size={18} /></button></div>
        <div className="nav-group">
          <span className="nav-caption">ESPAÇO DE TRABALHO</span>
          <button className={"nav-item" + (view === "catalog" && !selectedFolder ? " active" : "")}
            onClick={() => { setView("catalog"); setSelectedFolder(""); setSideOpen(false); }}>
            <Grid2X2 size={18} strokeWidth={1.9}/> Minha biblioteca <span className="nav-count">{collection?.files.length || 0}</span>
          </button>
        </div>
        <div className="folders-header"><span className="nav-caption">PASTAS</span><button className="icon-button small" title="Criar pasta" onClick={() => { setFolderParent(selectedFolder); setModal("folder"); }}><Plus size={18} /></button></div>
        <div className="folder-tree-search"><Search size={14}/><input aria-label="Filtrar pastas" placeholder="Localizar pasta…" value={folderQuery} onChange={(event) => setFolderQuery(event.target.value)}/>{folderQuery && <button aria-label="Limpar filtro de pastas" onClick={() => setFolderQuery("")}><X size={13}/></button>}</div>
        <div className="folder-scroller" role="tree" aria-label="Estrutura da biblioteca">
          <button className={"folder-row" + (view === "catalog" && !selectedFolder ? " active" : "") + (dragTarget === "/" ? " drop-target" : "")} onClick={() => { setView("catalog"); setSelectedFolder(""); setSideOpen(false); }}
            onDragOver={(event) => { if (event.dataTransfer.types.includes("text/acervo-book")) { event.preventDefault(); setDragTarget("/"); } }}
            onDragLeave={() => setDragTarget("")}
            onDrop={(event) => { event.preventDefault(); queueMove(dropPaths(event.dataTransfer), ""); }}>
            <FolderTree size={17} /> Raiz da biblioteca
          </button>
          {visibleFolders.map((folder) => (
            <div key={folder} role="treeitem" aria-expanded={folderParents.has(folder) ? expandedFolders.has(folder) : undefined}
              className={"folder-row" + (selectedFolder === folder ? " active" : "") + (dragTarget === folder ? " drop-target" : "")}
              style={{ paddingLeft: 7 + Math.min(6, folder.split("/").length - 1) * 13 }} title={folder}
              onDragOver={(event) => { if (event.dataTransfer.types.includes("text/acervo-book")) { event.preventDefault(); setDragTarget(folder); } }}
              onDragLeave={() => setDragTarget("")}
              onDrop={(event) => { event.preventDefault(); queueMove(dropPaths(event.dataTransfer), folder); }}>
              {folderParents.has(folder) ? <button className="folder-expander" aria-label={expandedFolders.has(folder) ? "Recolher pasta" : "Expandir pasta"} onClick={() => setExpandedFolders((prev) => {
                const next = new Set(prev); if (next.has(folder)) next.delete(folder); else next.add(folder); return next;
              })}>{expandedFolders.has(folder) ? <ChevronDown size={14}/> : <ChevronRight size={14}/>}</button> : <span className="folder-expander-spacer"/>}
              <button className="folder-select" onClick={() => { setSelectedFolder(folder); setView("catalog"); setSideOpen(false); if (folderParents.has(folder)) setExpandedFolders((prev) => new Set(prev).add(folder)); }}>
                <Folder size={16} strokeWidth={1.8}/><span>{folder.split("/").slice(-1)[0]}</span><small className="tree-count">{folderFileCounts.get(folder) || 0}</small>
              </button>
            </div>
          ))}
        </div>
        <div className="sidebar-bottom"><button className={"nav-item settings-nav" + (view === "settings" ? " active" : "")} onClick={() => { setView("settings"); setSelectedFolder(""); setSideOpen(false); }}><Settings2 size={18}/> Configurações</button><button className="library-path" onClick={() => void chooseRoot()} title="Escolher a raiz da biblioteca pelo explorador"><HardDrive size={18} /><span><strong>Biblioteca local</strong><small title={collection?.root || "Nenhuma pasta"}>{collection?.root || "Selecionar pasta"}</small></span><FolderOpen size={18} /></button><div className="sidebar-status"><span className="status-dot" /> Biblioteca offline · seus arquivos locais</div></div>
      </aside>
      <div className="sidebar-resizer" role="separator" aria-label="Redimensionar barra lateral" aria-orientation="vertical" tabIndex={0}
        aria-valuemin={210} aria-valuemax={450} aria-valuenow={sidebarWidth} title="Arraste para redimensionar · duplo clique para restaurar"
        onPointerDown={(event) => { if (event.button !== 0) return; sidebarResize.current = { x: event.clientX, width: sidebarWidth }; event.currentTarget.setPointerCapture(event.pointerId); }}
        onPointerMove={(event) => { if (!sidebarResize.current) return; setSidebarWidth(Math.min(450, Math.max(210, sidebarResize.current.width + event.clientX - sidebarResize.current.x))); }}
        onPointerUp={(event) => { sidebarResize.current = null; if (event.currentTarget.hasPointerCapture(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId); }}
        onPointerCancel={() => { sidebarResize.current = null; }}
        onDoubleClick={() => setSidebarWidth(254)}
        onKeyDown={(event) => { if (event.key === "ArrowLeft" || event.key === "ArrowRight") { event.preventDefault(); setSidebarWidth((width) => Math.min(450, Math.max(210, width + (event.key === "ArrowRight" ? 16 : -16)))); } }}
      />
      <div className="workspace">
        <header className="topbar glass-panel">
          <button className="icon-button mobile-menu" onClick={() => setSideOpen(true)} aria-label="Abrir menu"><Menu size={20} /></button>
          <div className="topbar-search"><Search size={18} /><input ref={searchRef} value={search} onChange={(event) => { setSearch(event.target.value); setActiveSuggestion(-1); }} onFocus={() => setSearchFocused(true)} onBlur={() => setSearchFocused(false)}
            onKeyDown={(event) => {
              if (event.key === "ArrowDown" && suggestions.length) { event.preventDefault(); setActiveSuggestion((i) => (i + 1) % suggestions.length); }
              if (event.key === "ArrowUp" && suggestions.length) { event.preventDefault(); setActiveSuggestion((i) => i <= 0 ? suggestions.length - 1 : i - 1); }
              if (event.key === "Enter" && activeSuggestion >= 0 && suggestions[activeSuggestion]) { event.preventDefault(); chooseSuggestion(suggestions[activeSuggestion]); }
              if (event.key === "Escape") setSearchFocused(false);
            }}
            placeholder="Pesquisar títulos, autores e pastas…" aria-label="Pesquisar no acervo" aria-autocomplete="list" aria-expanded={searchFocused && suggestions.length > 0} aria-controls="search-suggestions" />
            <kbd>Ctrl K</kbd>
            {searchFocused && suggestions.length > 0 && <div className="search-suggestions glass-panel" id="search-suggestions" role="listbox" aria-label="Sugestões de pesquisa">
              <div className="search-suggestions-heading" role="presentation">SUGESTÕES NO ACERVO <span>{suggestions.length}</span></div>
              {suggestions.map((suggestion, index) => <button role="option" aria-selected={activeSuggestion === index}
                className={activeSuggestion === index ? "is-active" : ""} key={suggestion.type + suggestion.value}
                onMouseEnter={() => setActiveSuggestion(index)}
                onMouseDown={(event) => event.preventDefault()} onClick={() => chooseSuggestion(suggestion)}>
                <span className="suggestion-kind">{suggestion.type}</span><span className="suggestion-label" title={suggestion.value}>{suggestion.value}</span><ArrowRight size={14}/>
              </button>)}
              <div className="search-suggestions-hint" role="presentation">↑ ↓ para navegar <span>Enter para selecionar</span></div>
            </div>}
          </div>
          <div className="topbar-actions">
            <button className="icon-button" title="Configurações" aria-label="Configurações" onClick={() => { setView("settings"); setSelectedFolder(""); }}><Settings2 size={19}/></button>
            <button className="icon-button" title="Atualizar catálogo" onClick={() => void refresh()}><RefreshCw className={loading ? "spinning" : ""} size={19} /></button>
          </div>
        </header>
        <main className="main-area">
          {view === "settings" ? <section className="settings-page">
            {error && <div className="alert" role="alert"><CircleHelp size={18}/><span>{error}</span><button onClick={() => setError("")} aria-label="Dispensar erro"><X size={16}/></button></div>}
            <div className="page-intro"><div><div className="eyebrow"><span className="eyebrow-dot"/> PREFERÊNCIAS</div><h1>Configurações</h1><p>Personalize a aparência e escolha onde sua biblioteca está armazenada.</p></div></div>
            <div className="settings-grid">
              <article className="settings-card glass-panel">
                <div className="settings-card-heading"><Palette size={22}/><div><h2>Aparência</h2><p>Alterne entre superfícies translúcidas e Material 3.</p></div></div>
                <div className="style-choices" role="group" aria-label="Estilo da interface">
                  <button className={"style-choice " + (visualStyle === "glass" ? "chosen" : "")} onClick={() => setVisualStyle("glass")} aria-pressed={visualStyle === "glass"}><Layers3 size={22}/><strong>Liquid Glass</strong><small>Transparência, brilho e profundidade</small></button>
                  <button className={"style-choice " + (visualStyle === "material" ? "chosen" : "")} onClick={() => setVisualStyle("material")} aria-pressed={visualStyle === "material"}><Grid2X2 size={22}/><strong>Material 3</strong><small>Superfícies sólidas e tonais</small></button>
                </div>
                <label className="settings-field">Paleta de cores
                  <select aria-label="Paleta de cores" value={palette} onChange={(event) => setPalette(event.target.value as PaletteName)}><option value="noctalia">Automática · Noctalia</option><option value="jade">Jade</option><option value="indigo">Índigo</option><option value="rose">Rosé</option><option value="amber">Âmbar</option></select>
                </label>
                <div className="theme-sync"><span className={"status-dot " + (theme ? "" : "muted")}/><div><strong>{theme ? "Noctalia conectado" : "Aguardando paleta do Noctalia"}</strong><small>{theme ? "As cores acompanham as alterações do wallpaper automaticamente." : "Para sincronizar, habilite o template do Acervo no Noctalia."}</small></div></div>
              </article>
              <article className="settings-card glass-panel">
                <div className="settings-card-heading"><HardDrive size={22}/><div><h2>Biblioteca</h2><p>Escolha a pasta raiz pelo explorador do sistema.</p></div></div>
                <div className="current-root"><FolderOpen size={20}/><span>{collection?.root || "Nenhuma biblioteca selecionada"}</span></div>
                <div className="settings-actions"><button className="button primary" onClick={() => void chooseRoot()}><FolderOpen size={17}/> Procurar pasta</button><button className="button secondary" onClick={() => setModal("root")}>Informar caminho</button></div>
                <p className="settings-footnote">Abrir uma biblioteca não altera arquivos. Movimentações exigem confirmação e não substituem arquivos existentes.</p>
              </article>
            </div>
            <div className="settings-version">Acervo 0.1.0-alpha.1 · dados locais · <span>atualizações experimentais</span></div>
          </section> : <>
          <div className="page-intro"><div><div className="eyebrow"><span className="eyebrow-dot"/> GESTOR DE BIBLIOTECA</div><h1>{currentLabel}</h1><p>Seus livros e sua lista de obras, organizados em um só lugar.</p></div><button className="button primary add-button" onClick={() => setModal("import")}><Plus size={18}/> Importar lista</button></div>
          <section className="stats-grid" aria-label="Resumo da biblioteca">
            <div className="stat-card glass-panel"><div className="stat-icon"><Book size={19}/></div><span>Arquivos disponíveis</span><strong>{collection?.files.length ?? "—"}</strong><small>PDF, EPUB e outros</small></div>
            <div className="stat-card glass-panel"><div className="stat-icon pink"><FilePlus2 size={19}/></div><span>Na lista de desejos</span><strong>{missing}</strong><small>Com pasta de destino planejada</small></div>
            <div className="stat-card glass-panel"><div className="stat-icon blue"><Folder size={19}/></div><span>Pastas organizadas</span><strong>{collection?.folders.length ?? "—"}</strong><small>Hierarquia da biblioteca</small></div>
          </section>
          <div className="quick-actions"><button onClick={() => setModal("prompt")}><Sparkles size={18}/> Gerar lista com IA <ArrowRight size={16}/></button><button onClick={() => { setFolderParent(selectedFolder); setModal("folder"); }}><FolderPlus size={18}/> Nova pasta <ArrowRight size={16}/></button><button onClick={() => setModal("import")}><Upload size={18}/> Importar JSON <ArrowRight size={16}/></button></div>
          {error && <div className="alert" role="alert"><CircleHelp size={18}/><span>{error}</span><button onClick={() => setError("")} aria-label="Dispensar erro"><X size={16}/></button></div>}
          {collection?.warnings.length ? <div className="warning">{collection.warnings.slice(0, 2).join(" · ")}</div> : null}
          {selectedFolder && <nav className="breadcrumbs" aria-label="Caminho da pasta"><button onClick={() => { setSelectedFolder(""); setView("catalog"); }}><Home size={15}/> Raiz</button>{breadcrumbs.map((crumb) => <span key={crumb.path}><ChevronRight size={14}/><button onClick={() => setSelectedFolder(crumb.path)}>{crumb.label}</button></span>)}</nav>}
          {subfolders.length > 0 && <section className="folder-grid" aria-label="Subpastas">{subfolders.map((folder) => (
            <button className="folder-tile glass-panel" key={folder} onClick={() => { setSelectedFolder(folder); setView("catalog"); setExpandedFolders((current) => new Set(current).add(folder)); }}>
              <Folder size={23}/><span><strong>{folder.split("/").slice(-1)[0]}</strong><small>{folderFileCounts.get(folder) || 0} livros dentro</small></span><ChevronRight size={17}/>
            </button>
          ))}</section>}
          <div className="content-top"><div><h2>Acervo <span>{filtered.length}</span></h2><p>Arraste um arquivo para uma pasta na lateral. Suas obras faltantes permanecem na lista.</p></div><div className="collection-controls"><div className="pill-control availability-control"><Filter size={16}/><select aria-label="Mostrar livros" value={availability} onChange={(event) => setAvailability(event.target.value as Availability)}><option value="all">Todos os livros</option><option value="available">Disponíveis</option><option value="missing">Lista de desejos</option></select><ChevronDown size={14}/></div><button className={"selection-toggle" + (selectionMode ? " active" : "")} onClick={() => { setSelectionMode(!selectionMode); setBulkSelected(new Set()); }}>{selectionMode ? <Check size={16}/> : <List size={16}/>} {selectionMode ? "Concluir" : "Selecionar"}</button><div className="pill-control"><Filter size={16}/><select aria-label="Filtrar formato" value={format} onChange={(event) => setFormat(event.target.value)}>{formatOptions.map((item) => <option key={item}>{item}</option>)}</select><ChevronDown size={14}/></div><div className="pill-control sort-control"><ArrowUpDown size={16}/><select aria-label="Ordenar livros" value={sortBy} onChange={(event) => setSortBy(event.target.value as SortBy)}><option value="title">Título</option><option value="author">Autor</option><option value="format">Formato</option><option value="size">Tamanho</option></select><ChevronDown size={14}/></div><div className="pill-control group-control"><Layers3 size={15}/><select aria-label="Agrupar livros" value={groupBy} onChange={(event) => setGroupBy(event.target.value as GroupBy)}><option value="none">Sem grupos</option><option value="folder">Por pasta</option><option value="format">Por formato</option><option value="author">Por autor</option></select><ChevronDown size={14}/></div><div className="view-switch" role="group" aria-label="Modo de visualização"><button className={layout === "grid" ? "chosen" : ""} title="Grade" onClick={() => setLayout("grid")}><Grid2X2 size={17}/></button><button className={layout === "list" ? "chosen" : ""} title="Lista" onClick={() => setLayout("list")}><List size={17}/></button></div></div></div>
          {bulkSelected.size > 0 && <div className="bulk-toolbar glass-panel"><div><Check size={17}/><strong>{bulkSelected.size} selecionado(s)</strong><small>Arraste para uma pasta ou escolha um destino.</small></div><button className="button primary" onClick={() => queueMove([...bulkSelected], selectedFolder || "")}><FolderOpen size={17}/> Mover para…</button><button className="icon-button" onClick={() => setBulkSelected(new Set())} title="Limpar seleção" aria-label="Limpar seleção"><X size={17}/></button></div>}
          {!collection ? <div className="empty-state glass-panel"><Archive size={42}/><h3>Seu acervo, no seu controle.</h3><p>Selecione uma pasta para visualizar os livros e o catálogo existente. Nada será alterado ao abrir.</p><button className="button primary" onClick={() => void chooseRoot()}>Escolher pelo explorador <FolderOpen size={16}/></button></div>
          : filtered.length ? <><div className={"books-" + layout}>{visibleEntries.map((entry, index) => (
            <Fragment key={entry.key}>
              {groupBy !== "none" && (index === 0 || groupLabel(entry) !== groupLabel(visibleEntries[index - 1])) &&
                <div className="book-group-heading"><span>{groupLabel(entry)}</span><span className="book-group-rule"/></div>}
            <button className={"book-card glass-panel" + (selection?.key === entry.key ? " selected" : "") + (bulkSelected.has(entry.file?.path || "") ? " bulk-selected" : "")} key={entry.key}
              aria-pressed={selectionMode && entry.file ? bulkSelected.has(entry.file.path) : undefined}
              onClick={(event) => { if (entry.file && (selectionMode || event.ctrlKey || event.metaKey)) toggleBookSelection(entry.file.path); else setSelection(entry); }} draggable={Boolean(entry.file)}
              onDragStart={(event) => { if (!entry.file) return; const paths = bulkSelected.has(entry.file.path) ? [...bulkSelected] : [entry.file.path]; event.dataTransfer.setData("text/acervo-book", entry.file.path); event.dataTransfer.setData("text/acervo-books", JSON.stringify(paths)); event.dataTransfer.effectAllowed = "move"; }}>
              {selectionMode && entry.file && <span className="book-check">{bulkSelected.has(entry.file.path) && <Check size={12}/>}</span>}
              <div className={"book-cover " + (entry.present ? "present" : "missing")}><span className="book-cover-decoration" /><Book size={26} strokeWidth={1.35}/><span>{entry.format}</span></div>
              <div className="book-info"><strong title={entry.title}>{entry.title}</strong><span>{entry.author || (entry.present ? "Arquivo local" : "Autor não informado")}</span><div className="book-meta"><span className={"status-badge " + (entry.present ? "available" : "wanted")}>{entry.present ? "Disponível" : "Faltante"}</span><span className="folder-path" title={entry.folder}>{entry.folder || "Raiz"}</span></div></div><ChevronRight className="book-chevron" size={17}/></button>
            </Fragment>
          ))}</div>{visibleLimit < filtered.length && <button className="load-more" onClick={() => setVisibleLimit((current) => current + 80)}>Mostrar mais livros <span>{visibleEntries.length} de {filtered.length}</span><ChevronDown size={16}/></button>}</> : <div className="empty-state glass-panel"><Search size={35}/><h3>Nada por aqui</h3><p>Altere os filtros ou importe uma lista de livros.</p></div>}
          {status && <div className="inline-status" role="status"><Check size={16}/>{status}</div>}
          </>}
        </main>
      </div>
      {selection && <div className="drawer-backdrop" onClick={() => setSelection(null)}><aside className="details-drawer glass-panel" onClick={(event) => event.stopPropagation()}><div className="drawer-head"><span className="eyebrow">DETALHES DA OBRA</span><button className="icon-button" onClick={() => setSelection(null)} aria-label="Fechar detalhes"><X size={19}/></button></div><div className={"drawer-cover " + (selection.present ? "present" : "missing")}><Book size={45}/></div><h2>{selection.title}</h2><p className="drawer-author">{selection.author || "Sem autor cadastrado"}</p><div className="detail-list"><div><span>Situação</span><strong>{selection.present ? "Disponível" : "Na lista de desejos"}</strong></div><div><span>Formato</span><strong>{selection.format}</strong></div><div><span>Pasta planejada</span><strong>{selection.folder || "Raiz"}</strong></div>{selection.file && <div><span>Arquivo</span><strong>{selection.file.path}</strong></div>}{selection.file && <div><span>Tamanho</span><strong>{readableBytes(selection.file.bytes)}</strong></div>}</div>{!selection.present && <div className="search-source"><h3>Buscar edição</h3><p>A pesquisa abre no navegador. Confira a origem e os direitos de uso antes de baixar.</p><label>Formato <select value={sourceFormat} onChange={(event) => setSourceFormat(event.target.value)}><option value="">Qualquer formato</option><option value="epub">EPUB</option><option value="pdf">PDF</option><option value="mobi">MOBI</option></select></label><label>Idioma <select value={sourceLanguage} onChange={(event) => setSourceLanguage(event.target.value)}>{languageOptions.map((item) => <option key={item.value} value={item.value}>{item.label}</option>)}</select></label><button className="button primary full" onClick={() => searchAnna(selection)}><Search size={17}/> Pesquisar no Anna's Archive</button></div>}<button className="button secondary full" onClick={() => void copy(selection.folder || ".", "Destino copiado para a área de transferência.")}><ClipboardCopy size={17}/> Copiar pasta de destino</button></aside></div>}
      {modal && <div className="modal-backdrop" onMouseDown={(event) => { if (event.target === event.currentTarget) setModal(null); }}><section className="modal glass-panel" role="dialog" aria-modal="true"><div className="modal-header"><div><span className="eyebrow">ACERVO</span><h2>{modal === "root" ? "Selecionar biblioteca" : modal === "prompt" ? "Gerador de listas" : modal === "folder" ? "Nova pasta" : modal === "move" ? "Mover livros" : "Importar lista JSON"}</h2></div><button className="icon-button" onClick={() => setModal(null)} aria-label="Fechar"><X size={20}/></button></div>
        {modal === "root" && <div className="modal-body"><p>Escolha a pasta raiz no explorador. O acervo será lido sem alterar livros ou pastas.</p><button className="button primary full" disabled={loading} onClick={() => void chooseRoot()}>{loading ? <LoaderCircle className="spinning" size={18}/> : <FolderOpen size={18}/>} Escolher pelo explorador</button><div className="manual-path"><label>Ou informe o caminho manualmente<input value={root} onChange={(event) => setRoot(event.target.value)} placeholder="/home/usuario/Books" /></label><button className="button secondary full" disabled={loading || !root.trim()} onClick={() => void refresh(root)}>Carregar caminho informado</button></div></div>}
        {modal === "prompt" && <div className="modal-body"><p>O prompt utiliza as pastas reais da sua biblioteca e produz um JSON compatível com o catálogo antigo.</p><div className="two-inputs"><label>Quantidade<input type="number" min={1} max={150} value={promptCount} onChange={(event) => setPromptCount(Math.min(150, Math.max(1, Number(event.target.value) || 1)))} /></label><label>Idioma<input value={promptLanguage} onChange={(event) => setPromptLanguage(event.target.value)} /></label></div><label>Temas e critérios<textarea value={promptTopics} onChange={(event) => setPromptTopics(event.target.value)} rows={2}/></label><label>Prévia do prompt<textarea readOnly value={createPrompt()} rows={11} /></label><button className="button primary full" onClick={() => void copy(createPrompt(), "Prompt copiado. Cole na IA de sua preferência.")}><ClipboardCopy size={17}/> Copiar prompt</button></div>}
        {modal === "import" && <div className="modal-body"><p>Cole uma lista JSON no formato <code>{'{ "versao": 1, "obras": [...] }'}</code>. Nesta primeira versão a importação é validada antes de salvar.</p><label>Conteúdo JSON<textarea value={importText} onChange={(event) => { setImportText(event.target.value); setImportNotice(""); }} rows={11} placeholder='{"versao":1,"obras":[{"titulo":"O Banquete","local_indicado":"Literatura/Filosofia"}]}' /></label>{importNotice && <div className="import-notice">{importNotice}</div>}<button className="button primary full" onClick={() => { try { const json = JSON.parse(importText); const works = Array.isArray(json) ? json : json.obras; if (!Array.isArray(works)) throw Error("Falta a lista obras."); if (works.some((item) => !item || typeof item.titulo !== "string" || !item.titulo.trim())) throw Error("Todas as obras precisam de um título."); setImportNotice(works.length + " obras válidas. Próxima etapa: gravação transacional no catálogo."); } catch (failure) { setImportNotice("Erro: " + String(failure)); } }}><Check size={17}/> Validar lista</button></div>}
        {modal === "folder" && <div className="modal-body"><p>Crie pastas dentro de qualquer nível da biblioteca. O caminho de destino será preservado.</p><label>Dentro de<select value={folderParent} onChange={(event) => setFolderParent(event.target.value)}><option value="">Raiz da biblioteca</option>{collection?.folders.map((folder) => <option key={folder} value={folder}>{folder}</option>)}</select></label><label>Nome da pasta<input value={newFolder} onChange={(event) => setNewFolder(event.target.value)} placeholder="Ex.: Filosofia" /></label><button className="button primary full" disabled={!collection || !newFolder.trim()} onClick={() => void addFolder()}><FolderPlus size={17}/> Criar pasta</button></div>}
        {modal === "move" && pendingMove && <div className="modal-body">
          <p>Revise a movimentação. Arquivos com o mesmo nome no destino não serão sobrescritos, e as referências no catálogo serão atualizadas.</p>
          <div className="move-summary"><strong>{pendingMove.paths.length} livro(s) selecionado(s)</strong><span>{pendingMove.paths.slice(0, 5).map((path) => <small key={path}>{path}</small>)}{pendingMove.paths.length > 5 && <small>e mais {pendingMove.paths.length - 5}…</small>}</span></div>
          <label>Pasta de destino<select value={pendingMove.destination} onChange={(event) => setPendingMove({ ...pendingMove, destination: event.target.value })}><option value="">Raiz da biblioteca</option>{collection?.folders.map((folder) => <option key={folder} value={folder}>{folder}</option>)}</select></label>
          {moveConflicts.length > 0 && <div className="warning">Há {moveConflicts.length} conflito(s): arquivo já presente no destino, nome repetido ou livro já nesta pasta. Selecione outro destino.</div>}
          {error && <div className="alert" role="alert">{error}</div>}
          <div className="move-actions"><button className="button secondary" onClick={() => setModal(null)}>Cancelar</button><button className="button primary" disabled={loading || moveConflicts.length > 0} onClick={() => void commitMove()}>{loading ? <LoaderCircle size={17} className="spinning"/> : <FolderOpen size={17}/>} Confirmar movimentação</button></div>
        </div>}
      </section></div>}
    </div>
  );
}

export default App;
