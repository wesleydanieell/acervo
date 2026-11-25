import { Fragment, useEffect, useMemo, useRef, useState, type MouseEvent as ReactMouseEvent } from "react";
import {
  Archive, ArrowDownToLine, ArrowRight, ArrowUpDown, Book, Check, ChevronDown, ChevronRight,
  CircleHelp, ClipboardCopy, Filter, Folder, FolderOpen,
  FolderPlus, FolderTree, Globe2, Grid2X2, HardDrive, Home, Layers3, List, LoaderCircle, Menu,
  Palette, Plug, Plus, RefreshCw, Link2, Search, Settings2, X,
} from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { ask, open, save } from "@tauri-apps/plugin-dialog";
import "./App.css";
import "./theme.css";
import "./explorer.css";
import "./catalog.css";
import "./context-menu.css";
import "./organization.css";
import "./workspace.css";
import { nextMonitorGeneration } from "./library-monitor-state";
import { ManualMetadataEditor } from "./ManualMetadataEditor";
import { effectiveMetadata, metadataFields, type ManualMetadata } from "./manual-metadata";
import { DuplicatesCenter } from "./DuplicatesCenter";
import { MetadataCenter, type CachedMetadata } from "./MetadataCenter";
import { SmartOrganization } from "./SmartOrganization";
import { CatalogCenter } from "./CatalogCenter";
import { SourcesCenter } from "./SourcesCenter";
import { namingOptions, readNamingPreset, type NamingPreset } from "./download-naming";
import { DownloadsCenter } from "./DownloadsCenter";
import type { DiscoveryRequest } from "./download-workspace";
import { BookCover } from "./BookCover";
import { VirtualBookList } from "./VirtualBookList";
import "./virtual-list.css";
import {usePaletteMotion} from "./usePaletteMotion";
import type {PaletteName,ThemeTokens} from "./palette-motion";
import { alternateEditions, indexEditions } from "./edition-matches";
import { buildCatalogPrompt } from "./ai-catalog-prompt";
import { selectVisibleRange } from "./library-selection";
import { chosenPromptFolders, isServiceCatalogPath } from "./prompt-folder-selection";
import { PromptFolderPicker } from "./PromptFolderPicker";
import acervoIcon from "../branding/acervo-icon.svg";
import { ActionMenu } from "./ActionMenu";
import { ContextMenu, type ContextAction } from "./ContextMenu";

type LibraryEdition = {path:string;sourceName:string;metadata:{title:string;author:string;language:string;publisher:string;editionLabel:string;remoteId?:string|null;sourceId:string};fingerprint:{bytes:number;sha256:string}};
type BookFile = { id: string; title: string; path: string; folder: string; format: string; bytes: number; edition?: LibraryEdition; embedded?: CachedMetadata; manual?:{metadata:ManualMetadata} };
type CatalogWork = { id: number; title: string; author: string; publishers: string; destination: string; present: boolean; priority: boolean; associatedFiles: string[] };
type Collection = { root: string; files: BookFile[]; folders: string[]; works: CatalogWork[]; warnings: string[] };
type View = "catalog" | "lists" | "settings" | "sources" | "extensions" | "downloads";
type GroupBy = "none" | "folder" | "format" | "author";
type Layout = "grid" | "list";
type Entry = { key: string; type: "file" | "work"; title: string; author: string; format: string; folder: string; present: boolean; file?: BookFile; work?: CatalogWork };

type VisualStyle = "glass" | "material" | "workspace";
type SortBy = "title" | "author" | "format" | "size";

type MenuState = { x: number; y: number; kind: "folder" | "file" | "background"; path: string };
type OrganizationPreview = { items: { source: string; destination: string; target: string; bytes: number }[]; totalBytes: number };

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
      key: file.id + ":" + file.path, type: "file", title: effectiveMetadata(file).title, author: effectiveMetadata(file).author, format: file.format, folder: file.folder, present: true, file,
    })),
  ].sort((a, b) => a.title.localeCompare(b.title, "pt-BR"));
}

function App() {
  const [root, setRoot] = useState(() => localStorage.getItem("acervo-root") || "");
  const [collection, setCollection] = useState<Collection | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");
  const [view, setView] = useState<View>("catalog");
  const [groupBy, setGroupBy] = useState<GroupBy>("none");
  const [sidebarWidth, setSidebarWidth] = useState(() => Math.min(450, Math.max(210, Number(localStorage.getItem("acervo-sidebar-width")) || 254)));
  const sidebarResize = useRef<{ x: number; width: number } | null>(null);
  const [searchFocused, setSearchFocused] = useState(false);
  const [folderQuery, setFolderQuery] = useState("");
  const [activeSuggestion, setActiveSuggestion] = useState(-1);
  const [namingPreset,setNamingPreset]=useState<NamingPreset>(()=>readNamingPreset(localStorage.getItem("acervo-naming-preset")));
  useEffect(()=>{localStorage.setItem("acervo-naming-preset",namingPreset);},[namingPreset]);
  const [layout, setLayout] = useState<Layout>(() => localStorage.getItem("acervo-layout") === "list" ? "list" : "grid");
  useEffect(() => { localStorage.setItem("acervo-layout", layout); }, [layout]);
  const [sortBy, setSortBy] = useState<SortBy>("title");
  const [selectedFolder, setSelectedFolder] = useState("");
  const [search, setSearch] = useState("");
  const [format, setFormat] = useState("Todos");
  const [selection, setSelection] = useState<Entry | null>(null);
  const [associationError, setAssociationError] = useState("");
  const [associationBusy, setAssociationBusy] = useState(false);
  const associationLock = useRef(false);
  const currentCollectionRoot = useRef("");
  currentCollectionRoot.current = collection?.root || "";
  useEffect(() => setAssociationError(""), [selection?.key]);
  useEffect(() => { setSelection(null); }, [view, selectedFolder]);
  const [modal, setModal] = useState<"root" | "prompt" | "import" | "folder" | "move" | "organize" | null>(null);
  const [sideOpen, setSideOpen] = useState(false);
  const [palette, setPalette] = useState<PaletteName>(() => (localStorage.getItem("acervo-palette") as PaletteName) || "noctalia");
  const [visualStyle, setVisualStyle] = useState<VisualStyle>(() => { const saved = localStorage.getItem("acervo-workspace-style"); return saved === "material" || saved === "glass" ? saved : "workspace"; });
  const [theme, setTheme] = useState<ThemeTokens | null>(null);
  const searchRef = useRef<HTMLInputElement>(null);
  const mainRef=useRef<HTMLElement>(null);
  useEffect(()=>{
    const main=mainRef.current;
    if(!main)return;
    main.scrollTop=0;
    if(window.matchMedia("(prefers-reduced-motion: reduce)").matches||typeof main.animate!=="function")return;
    const animation=main.animate([{opacity:.65},{opacity:1}],{duration:180,easing:"cubic-bezier(.2,0,0,1)"});
    return()=>animation.cancel();
  },[view]);

  const [importText, setImportText] = useState("");
  const [importNotice, setImportNotice] = useState("");
  const [promptCount, setPromptCount] = useState(10);
  const [promptTopics, setPromptTopics] = useState("Filosofia, literatura clássica e brasileira");
  const [promptLanguage, setPromptLanguage] = useState("Português");
  const [promptFolders, setPromptFolders] = useState<Set<string> | null>(null);
  const [promptDraft, setPromptDraft] = useState<string | null>(null);
  const [sourceFormat, setSourceFormat] = useState("");
  const [sourceLanguage, setSourceLanguage] = useState("");
  const [sourceRequest, setSourceRequest] = useState<DiscoveryRequest | null>(null);
  const [newFolder, setNewFolder] = useState("");
  const [folderParent, setFolderParent] = useState("");
  const [contextMenu, setContextMenu] = useState<MenuState | null>(null);
  const [smartRoot,setSmartRoot]=useState("");
  const [metadataRoot,setMetadataRoot]=useState("");
  const [manualEdit,setManualEdit]=useState<{root:string;file:BookFile}|null>(null);
  const [duplicateRoot,setDuplicateRoot]=useState("");
  const [organizationText, setOrganizationText] = useState("");
  const [organizationPreview, setOrganizationPreview] = useState<OrganizationPreview | null>(null);
  const [organizationConfirmed, setOrganizationConfirmed] = useState(false);
  const [organizationError, setOrganizationError] = useState("");
  const [status, setStatus] = useState("");
  const [dragTarget, setDragTarget] = useState("");
  const [expandedFolders, setExpandedFolders] = useState<Set<string>>(() => new Set());
  const [selectionMode, setSelectionMode] = useState(false);
  const [bulkSelected, setBulkSelected] = useState<Set<string>>(() => new Set());
  const selectionAnchor = useRef<string | null>(null);
  const [pendingMove, setPendingMove] = useState<{ paths: string[]; destination: string } | null>(null);
  const [visibleLimit, setVisibleLimit] = useState(80);
  useEffect(() => { localStorage.setItem("acervo-sidebar-width", String(sidebarWidth)); }, [sidebarWidth]);

  const refreshGeneration = useRef(0);
  const requestedRoot = useRef(root);
  const manualRefresh = useRef(false);
  const [monitorEnabled,setMonitorEnabled]=useState(()=>localStorage.getItem("acervo-folder-monitor")!=="off");
  const [monitorError,setMonitorError]=useState("");
  const monitorEnabledRef=useRef(monitorEnabled);
  monitorEnabledRef.current=monitorEnabled;
  const monitoredRoot=collection?.root??"";
  useEffect(()=>{
    localStorage.setItem("acervo-folder-monitor",monitorEnabled?"on":"off");
    setMonitorError("");
    let active=true;
    void invoke("set_library_monitor",{root:monitorEnabled&&monitoredRoot?monitoredRoot:null,generation:nextMonitorGeneration()}).catch(failure=>{if(active)setMonitorError(String(failure));});
    return()=>{active=false;};
  },[monitorEnabled,monitoredRoot]);
  useEffect(() => {
    let disposed=false;
    let pending: {root:string;changes:string[]|null}|null=null;
    let running=false;
    let timer:number|undefined;
    const schedule=()=>{
      if(disposed||timer!==undefined||running||!pending)return;
      timer=window.setTimeout(()=>{timer=undefined;void drain();},500);
    };
    const drain=async()=>{
      if(disposed||running||!pending)return;
      if(manualRefresh.current){schedule();return;}
      const batch=pending;pending=null;
      const payload=batch.root;
      if(payload!==currentCollectionRoot.current||payload!==requestedRoot.current)return;
      running=true;
      const generation=refreshGeneration.current;
      try{
        const data=await invoke<Collection>("refresh_indexed_collection",{root:payload,changes:batch.changes});
        if(!disposed&&generation===refreshGeneration.current&&payload===requestedRoot.current&&payload===currentCollectionRoot.current){
          setCollection(data);setSelection(null);
        }
      }catch(failure){if(!disposed&&generation===refreshGeneration.current)setError(String(failure));}
      finally{running=false;schedule();}
    };
    const changed=(root:string,changes:string[]|null)=>{
      if(disposed||root!==currentCollectionRoot.current)return;
      if(pending?.root===root){
        if(pending.changes===null||changes===null)pending={root,changes:null};
        else{
          const combined=[...new Set([...pending.changes,...changes])];
          pending={root,changes:combined.length>128?null:combined};
        }
      }else pending={root,changes};
      schedule();
    };
    const subscriptions=[listen<string>("acervo-library-changed",event=>changed(event.payload,null)),listen<{root:string;changes:string[]|null}>("acervo-library-fs-changed",event=>{if(monitorEnabledRef.current)changed(event.payload.root,event.payload.changes);})];
    return()=>{disposed=true;if(timer!==undefined)window.clearTimeout(timer);for(const subscription of subscriptions)void subscription.then(unlisten=>unlisten()).catch(()=>{});};
  }, []);

  const refresh = async (location = root, initial = false) => {
    if (!location.trim()) { setModal("root"); return; }
    const generation = ++refreshGeneration.current;
    manualRefresh.current=true;
    requestedRoot.current = location.trim();
    setLoading(true); setError("");
    try {
      const data = await invoke<Collection>(initial ? "load_initial_collection" : "load_collection", { root: location.trim() });
      if (generation !== refreshGeneration.current) return;
      if (data.root !== root) { setPromptFolders(null); setPromptDraft(null); setSourceRequest(null); }
      requestedRoot.current = data.root; setCollection(data); setRoot(data.root); localStorage.setItem("acervo-root", data.root);
      setStatus(data.files.length + " arquivos encontrados; nenhuma alteração feita nos livros.");
      setModal(null);
    } catch (failure) { if (generation === refreshGeneration.current) { requestedRoot.current = currentCollectionRoot.current; setError(String(failure)); } }
    finally { if (generation === refreshGeneration.current) {manualRefresh.current=false;setLoading(false);} }
  };

  const chooseRoot = async () => {
    try {
      const chosen = await open({ directory: true, multiple: false, title: "Selecionar a pasta raiz do Acervo", defaultPath: collection?.root || undefined });
      if (typeof chosen === "string") await refresh(chosen);
    } catch (failure) { setError("Não foi possível abrir o explorador: " + String(failure)); }
  };

  useEffect(() => { if (root) void refresh(root, true); }, []); // Validate the persistent index on startup.
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
  useEffect(()=>{
    let cancelled=false,pending=false,misses=0;
    const read=async()=>{
      if(pending||cancelled)return;
      pending=true;
      try{
        const tokens=await invoke<ThemeTokens|null>("read_noctalia_colors");
        if(cancelled)return;
        if(tokens){misses=0;setTheme(previous=>JSON.stringify(previous)===JSON.stringify(tokens)?previous:tokens);}
        else if(++misses>=3)setTheme(null);
      }catch{if(!cancelled&&++misses>=3)setTheme(null);}
      finally{pending=false;}
    };
    void read();
    const timer=window.setInterval(read,1000);
    return()=>{cancelled=true;window.clearInterval(timer);};
  },[]);
  useEffect(()=>{
    localStorage.setItem("acervo-workspace-style",visualStyle);
    document.documentElement.dataset.visualStyle=visualStyle;
  },[visualStyle]);
  useEffect(()=>{localStorage.setItem("acervo-palette",palette);},[palette]);
  usePaletteMotion(palette,theme);

  const entries = useMemo(() => collection ? getEntries(collection) : [], [collection]);
  const editionIndex = useMemo(() => indexEditions(collection?.files || []), [collection]);
  const selectedAlternatives = selection?.work ? alternateEditions(selection.work, editionIndex) : [];
  // The file browser represents every physical file, even when editions share a catalog work.
  const libraryEntries = useMemo(() => {
    if (!collection) return [] as Entry[];
    const byFile = new Map(collection.works.flatMap((work) => work.associatedFiles.map((path) => [path, work] as const)));
    return collection.files.map((file): Entry => {
      const work = byFile.get(file.path);
      return { key: "file:" + file.path, type: "file", title: file.manual?.metadata.title || work?.title || effectiveMetadata(file).title,
        author: file.manual?.metadata.author || work?.author || effectiveMetadata(file).author, format: file.format, folder: file.folder, present: true, file, work };
    });
  }, [collection]);
  const searchNeedle = useMemo(() => fold(search), [search]);
  const librarySearchIndex = useMemo(() => new Map(libraryEntries.map(entry => [entry.key, fold([entry.title,entry.author,entry.folder,entry.file?.edition?.metadata.title,entry.file?.edition?.metadata.author,entry.file?.edition?.metadata.publisher,entry.file?.edition?.metadata.editionLabel,entry.file?.edition?.metadata.language,entry.file?.title,entry.file?.path,entry.file?.embedded?.metadata?.title,entry.file?.embedded?.metadata?.author,entry.file?.embedded?.metadata?.publisher,entry.file?.embedded?.metadata?.language,entry.file?.embedded?.metadata?.isbn,...Object.values(entry.file?.manual?.metadata??{})].filter(Boolean).join(" "))])), [libraryEntries]);
  const filtered = useMemo(() => libraryEntries.filter((entry) => {
    if (!entry.present) return false; // A biblioteca é um explorador real: nunca exibe desejos.
    if (selectedFolder && !(entry.folder === selectedFolder || entry.folder.startsWith(selectedFolder + "/"))) return false;
    if (format !== "Todos" && entry.format !== format) return false;
    if (searchNeedle && !librarySearchIndex.get(entry.key)?.includes(searchNeedle)) return false;
    return true;
  }).sort((a, b) => {
    if (sortBy === "size") return (b.file?.bytes || 0) - (a.file?.bytes || 0) || a.title.localeCompare(b.title, "pt-BR");
    const left = sortBy === "author" ? a.author : sortBy === "format" ? a.format : a.title;
    const right = sortBy === "author" ? b.author : sortBy === "format" ? b.format : b.title;
    return left.localeCompare(right, "pt-BR") || a.title.localeCompare(b.title, "pt-BR");
  }), [libraryEntries, librarySearchIndex, selectedFolder, format, searchNeedle, sortBy]);
  const breadcrumbs = selectedFolder.split("/").filter(Boolean).map((label, index, parts) => ({ label, path: parts.slice(0, index + 1).join("/") }));
  useEffect(() => { setVisibleLimit(80); if(mainRef.current) mainRef.current.scrollTop=0; }, [selectedFolder, format, search, sortBy, groupBy, layout, collection?.root]);
  const groupLabel = (entry: Entry) => groupBy === "folder" ? entry.folder || "Raiz" : groupBy === "format" ? entry.format : groupBy === "author" ? entry.author || "Sem autor" : "";
  const displayEntries = useMemo(() => groupBy === "none" ? filtered : [...filtered].sort((a, b) =>
    groupLabel(a).localeCompare(groupLabel(b), "pt-BR") || a.title.localeCompare(b.title, "pt-BR")
  ), [filtered, groupBy]);
  const virtualList = layout==="list"&&groupBy==="none"&&displayEntries.length>80;
  const visibleEntries = virtualList ? displayEntries : displayEntries.slice(0, visibleLimit);
  const suggestions = useMemo(() => {
    const needle = fold(search.trim());
    if (!needle || !searchFocused) return [] as { type: "Livro" | "Autor" | "Pasta"; value: string }[];
    const result: { type: "Livro" | "Autor" | "Pasta"; value: string }[] = [];
    const seen = new Set<string>();
    const add = (type: "Livro" | "Autor" | "Pasta", value: string) => {
      const key = type + ":" + fold(value);
      if (value && fold(value).includes(needle) && !seen.has(key)) { seen.add(key); result.push({ type, value }); }
    };
    const scope = view === "lists" ? entries.filter((entry) => entry.type === "work") : libraryEntries;
    for (const entry of scope) { add("Livro", entry.title); if (result.length >= 4) break; }
    for (const entry of scope) { add("Autor", entry.author); if (result.length >= 6) break; }
    if (view !== "lists") for (const folder of collection?.folders || []) { add("Pasta", folder); if (result.length >= 8) break; }
    return result.slice(0, 8);
  }, [search, searchFocused, entries, libraryEntries, collection, view]);
  const chooseSuggestion = (suggestion: { type: "Livro" | "Autor" | "Pasta"; value: string }) => {
    if (suggestion.type === "Pasta") { setSelectedFolder(suggestion.value); setSearch(""); }
    else setSearch(suggestion.value);
    if (suggestion.type === "Pasta" || view !== "lists") setView("catalog"); setSearchFocused(false); setActiveSuggestion(-1);
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
  const currentLabel = selectedFolder ? selectedFolder.split("/").slice(-1)[0] : "Minha biblioteca";
  const promptDirectoryOptions = useMemo(() => (collection?.folders || []).filter((folder) =>
    !isServiceCatalogPath(folder)
  ), [collection]);

  const createPrompt = () => {
    const selected = chosenPromptFolders(promptDirectoryOptions, promptFolders);
    const folders = [
      ...(promptFolders === null || !selected.length ? ["- . (raiz da biblioteca)"] : []),
      ...selected.map((folder) => "- " + folder),
    ].join("\n");
    const sample = { versao: 1, obras: [{ titulo: "O Banquete", autor: "Platão", editoras_indicadas: ["Editora de exemplo"], local_indicado: selected[0] || "", enfoques: "Filosofia", categorias: "Clássicos", prioridade_alta: 0 }] };
    return `Gere uma lista de ${promptCount} livros reais em JSON para importar no meu gerenciador de acervo.

ASSUNTOS: ${promptTopics}
IDIOMA PREFERIDO: ${promptLanguage}
CRITÉRIOS: sugira obras e editoras verificáveis, não invente edições; evite títulos repetidos.
DESTINOS SELECIONADOS (use apenas estes em local_indicado; se só houver a raiz, use uma string vazia):
${folders}
Já possuo ${collection?.files.length || 0} arquivos indexados e ${collection?.works.length || 0} obras cadastradas.

RESPONDA APENAS JSON VÁLIDO. Cada obra tem titulo (obrigatório), autor, editoras_indicadas (lista de strings), local_indicado (caminho RELATIVO, sem ../), enfoques, categorias e prioridade_alta (0/1). Não inclua URLs ou arquivos_encontrados se não forem conhecidos. Exemplo:
${JSON.stringify(sample, null, 2)}`;
  };

  const createOrganizationPrompt = () => {
    if (!collection) return "";
    const snapshot = {
      versao: 1,
      pastas: ["", ...collection.folders],
      arquivos: collection.files.slice(0, 5000).map((file) => ({
        arquivo: file.path, titulo: file.title, formato: file.format,
        tamanho_bytes: file.bytes, pasta_atual: file.folder,
      })),
    };
    return [
      "Você é um organizador de acervos. Analise o JSON abaixo e sugira movimentações somente quando fizer sentido.",
      "Responda APENAS um JSON válido no formato abaixo. Não invente livros, caminhos ou pastas.",
      "Use exclusivamente os caminhos de arquivo e as pastas que aparecem no inventário.",
      "Não renomeie arquivos; só indique novas pastas de destino que já existam.",
      "No máximo 100 movimentações por resposta; não inclua arquivos que já estão no destino.",
      'Formato: {"versao":1,"movimentacoes":[{"arquivo":"caminho/atual/livro.epub","destino":"Pasta/Existente","tamanho_bytes":12345}]}',
      "Atenção: nenhuma movimentação será aplicada sem prévia e confirmação manual no Acervo.",
      "INVENTÁRIO DA BIBLIOTECA (caminhos relativos):",
      JSON.stringify(snapshot, null, 2),
    ].join("\n\n");
  };
  const openOrganizationDialog = () => {
    setOrganizationText("");
    setOrganizationPreview(null);
    setOrganizationConfirmed(false);
    setOrganizationError("");
    setModal("organize");
  };
  const previewOrganization = async () => {
    if (!collection || !organizationText.trim()) return;
    setOrganizationError("");
    setOrganizationConfirmed(false);
    setOrganizationPreview(null);
    try {
      const preview = await invoke<OrganizationPreview>("preview_organization", {
        root: collection.root, jsonText: organizationText,
      });
      setOrganizationPreview(preview);
    } catch (failure) { setOrganizationError(String(failure)); }
  };
  const applyOrganization = async () => {
    if (!collection || !organizationPreview || !organizationConfirmed) return;
    setLoading(true); setOrganizationError("");
    try {
      const moved = await invoke<string[]>("apply_organization", {
        root: collection.root, jsonText: organizationText,
      });
      await refresh(collection.root);
      setBulkSelected(new Set()); setSelection(null); setOrganizationPreview(null);
      setStatus(moved.length + " arquivo(s) reorganizado(s) a partir do JSON.");
    } catch (failure) {
      setOrganizationError(String(failure));
      setOrganizationConfirmed(false);
    } finally { setLoading(false); }
  };

  const copy = async (text: string, message: string) => {
    try { await navigator.clipboard.writeText(text); setStatus(message); }
    catch { setStatus("Não foi possível copiar; selecione e copie manualmente."); }
  };
  const exportCsv = async (csv: string) => {
    setError("");
    try {
      const target = await save({
        title: "Exportar catálogo em CSV",
        defaultPath: "Acervo_Catalogo.csv",
        filters: [{ name: "Planilha CSV", extensions: ["csv"] }],
      });
      if (!target) return;
      const path = await invoke<string>("save_catalog_csv", { path: target, csvText: csv });
      setStatus("Catálogo CSV exportado para " + path);
    } catch (failure) { setError("Não foi possível exportar CSV: " + String(failure)); }
  };
  const associateExistingFile = async (work: CatalogWork, suggestedPath?: string) => {
    const libraryRoot = collection?.root;
    if (!libraryRoot || work.id >= 0 || associationLock.current) return;
    associationLock.current = true; setAssociationBusy(true); setAssociationError("");
    try {
      let relativePath = suggestedPath;
      if (!relativePath) {
        const chosen = await open({ multiple: false, directory: false, defaultPath: libraryRoot,
          title: "Selecionar edição existente", filters: [{ name: "Livros", extensions: ["pdf", "epub", "mobi", "azw3", "djvu"] }] });
        if (typeof chosen !== "string") return;
        const prefix = libraryRoot.endsWith("/") ? libraryRoot : libraryRoot + "/";
        if (!chosen.startsWith(prefix)) throw new Error("Selecione um arquivo dentro da biblioteca atual.");
        relativePath = chosen.slice(prefix.length);
      }
      if (!await ask("Associar " + relativePath + " à obra " + work.title + (work.author ? " — " + work.author : "") + "? O arquivo não será movido.", {
        title: "Associar edição", kind: "info",
      })) return;
      if (currentCollectionRoot.current !== libraryRoot) throw new Error("A biblioteca mudou; selecione o arquivo novamente.");
      await invoke("link_catalog_file", { root: libraryRoot, workId: work.id, title: work.title,
        author: work.author, destination: work.destination, relativePath });
      if (currentCollectionRoot.current === libraryRoot) {
        setSelection(null);
        await refresh(libraryRoot);
        setStatus("Arquivo associado à obra: " + work.title);
      }
    } catch (failure) { setAssociationError(String(failure)); }
    finally { associationLock.current = false; setAssociationBusy(false); }
  };

  const searchInInstalledSources = (entry: Entry) => {
    setSourceRequest({
      query: [entry.title, entry.author].filter(Boolean).join(" "),
      format: sourceFormat,
      language: sourceLanguage,
      destination: entry.folder === "." ? "" : entry.folder,
      catalogTarget: entry.work && collection ? {
        root: collection.root, title: entry.work.title, author: entry.work.author, destination: entry.work.destination, legacyId: entry.work.id > 0 ? entry.work.id : undefined,
      } : undefined,
    });
    setSelection(null);
    setView("sources");
    setSideOpen(false);
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
    selectionAnchor.current = path;
    setSelectionMode(true);
    setBulkSelected((current) => {
      const next = new Set(current);
      if (next.has(path)) next.delete(path); else next.add(path);
      return next;
    });
  };
  const selectBookRange = (path: string, extend: boolean) => {
    const visiblePaths = visibleEntries.flatMap((entry) => entry.file ? [entry.file.path] : []);
    const anchor = selectionAnchor.current;
    if (!anchor || !visiblePaths.includes(anchor)) selectionAnchor.current = path;
    setBulkSelected((current) => selectVisibleRange(visiblePaths, anchor, path, current, extend));
    setSelectionMode(true);
  };
  const dropPaths = (data: DataTransfer): string[] => {
    try {
      const many = JSON.parse(data.getData("text/acervo-books"));
      if (Array.isArray(many) && many.every((item) => typeof item === "string")) return many;
    } catch { /* A normal single-file drag has no JSON payload. */ }
    const single = data.getData("text/acervo-book");
    return single ? [single] : [];
  };
  const importBooks = async (jsonText = importText) => {
    if (!collection) { setImportNotice("Selecione primeiro uma biblioteca."); return; }
    setLoading(true); setImportNotice("");
    try {
      const report = await invoke<{ imported: number; skipped: number; total: number }>("import_catalog_json", {
        root: collection.root, jsonText,
      });
      await refresh(collection.root);
      setStatus(report.imported + " obra(s) adicionada(s) ao catálogo; " + report.skipped + " já cadastrada(s).");
      setImportNotice("");
      setImportText("");
      setModal(null);
    } catch (failure) { const message = "Falha na importação: " + String(failure); setImportNotice(message); setError(message); }
    finally { setLoading(false); }
  };

  const prepareAiCatalog = async (books: { path: string; title: string; folder: string; format: string }[]) => {
    try {
      await navigator.clipboard.writeText(buildCatalogPrompt(books, collection?.folders || []));
      setImportText("");
      setImportNotice("Prompt copiado. Envie à IA de sua preferência e cole abaixo o JSON resultante para importar as fichas. Os arquivos não serão movidos.");
      setModal("import");
    } catch (failure) { setError("Falha ao copiar o prompt para a área de transferência: " + String(failure)); }
  };

  const openFolderDialog = (parent: string) => {
    setNewFolder("");
    setFolderParent(parent);
    setContextMenu(null);
    setModal("folder");
  };
  const openContextMenu = (event: ReactMouseEvent, kind: MenuState["kind"], path: string) => {
    if (!collection) return;
    event.preventDefault();
    event.stopPropagation();
    setContextMenu({ x: event.clientX, y: event.clientY, kind, path });
  };
  const contextActions: ContextAction[] = !contextMenu ? [] : contextMenu.kind === "file" ? [
    { label: "Ver detalhes", icon: <Book size={16}/>, onSelect: () => {
      const entry = libraryEntries.find((candidate) => candidate.file?.path === contextMenu.path);
      if (entry) setSelection(entry);
    } },
    { label: "Selecionar livro", icon: <Check size={16}/>, onSelect: () => toggleBookSelection(contextMenu.path) },
    { label: "Mover para outra pasta…", icon: <FolderOpen size={16}/>, onSelect: () => queueMove([contextMenu.path], selectedFolder) },
    { label: "Copiar caminho", icon: <ClipboardCopy size={16}/>, onSelect: () => void copy([collection?.root, contextMenu.path].filter(Boolean).join("/"), "Caminho copiado.") },
  ] : [
    { label: "Abrir pasta", icon: <FolderOpen size={16}/>, onSelect: () => {
      setSelectedFolder(contextMenu.path); setView("catalog"); setSideOpen(false);
      setExpandedFolders((current) => new Set(current).add(contextMenu.path));
    } },
    { label: "Nova pasta aqui", icon: <FolderPlus size={16}/>, onSelect: () => openFolderDialog(contextMenu.path) },
    ...(bulkSelected.size > 0 ? [{ label: "Mover livros selecionados aqui", icon: <FolderOpen size={16}/>, onSelect: () => queueMove([...bulkSelected], contextMenu.path) }] : []),
    { label: "Copiar caminho", icon: <ClipboardCopy size={16}/>, onSelect: () => void copy([collection?.root, contextMenu.path].filter(Boolean).join("/"), "Caminho copiado.") },
  ];

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

  const renderBookCard=(entry:Entry,tabIndex=0)=> (
    <button className={"book-card glass-panel" + (selection?.key === entry.key ? " selected" : "") + (bulkSelected.has(entry.file?.path || "") ? " bulk-selected" : "")} key={entry.key} tabIndex={tabIndex}
              aria-pressed={selectionMode && entry.file ? bulkSelected.has(entry.file.path) : undefined}
              onContextMenu={(event) => { if (entry.file) openContextMenu(event, "file", entry.file.path); }}
              onClick={(event) => {
                if (!entry.file) { setSelection(entry); return; }
                if (event.shiftKey) selectBookRange(entry.file.path, event.ctrlKey || event.metaKey);
                else if (selectionMode || event.ctrlKey || event.metaKey) toggleBookSelection(entry.file.path);
                else { selectionAnchor.current = entry.file.path; setSelection(entry); }
              }} draggable={Boolean(entry.file)}
              onDragStart={(event) => { if (!entry.file) return; const paths = bulkSelected.has(entry.file.path) ? [...bulkSelected] : [entry.file.path]; event.dataTransfer.setData("text/acervo-book", entry.file.path); event.dataTransfer.setData("text/acervo-books", JSON.stringify(paths)); event.dataTransfer.effectAllowed = "move"; }}>
              {selectionMode && entry.file && <span className="book-check">{bulkSelected.has(entry.file.path) && <Check size={12}/>}</span>}
              <BookCover root={collection?.root || ""} file={entry.file} format={entry.format} present={entry.present} title={entry.title} />
              <div className="book-info"><strong title={entry.title}>{entry.title}</strong><span>{entry.author || "Autor não informado"}</span><div className="book-meta"><span className={"status-badge " + (entry.present ? "available" : "wanted")}>{entry.present ? entry.format : "Faltante"}</span><span className="folder-path" title={entry.folder}>{entry.folder || "Raiz"}</span></div></div><ChevronRight className="book-chevron" size={17}/></button>
  );

  return (
    <div className="app-shell">
      <div className="background-orb orb-a" /><div className="background-orb orb-b" />
      <aside className={"sidebar glass-panel" + (sideOpen ? " open" : "")} style={{ width: sidebarWidth }}>
        <div className="brand"><span className="brand-symbol"><img src={acervoIcon} alt="" aria-hidden="true" /></span><div><strong>Acervo</strong><small>Organização pessoal</small></div><button className="icon-button mobile-close" onClick={() => setSideOpen(false)} aria-label="Fechar menu"><X size={18} /></button></div>
        <nav className="nav-group" aria-label="Biblioteca">
          <span className="nav-caption">Biblioteca</span>
          <button aria-current={view === "catalog" && !selectedFolder ? "page" : undefined} className={"nav-item" + (view === "catalog" && !selectedFolder ? " active" : "")}
            onClick={() => { setView("catalog"); setSelectedFolder(""); setSideOpen(false); }}>
            <Grid2X2 size={18} strokeWidth={1.9}/> Minha biblioteca <span className="nav-count">{collection?.files.length || 0}</span>
          </button>
          <button aria-current={view === "lists" ? "page" : undefined} className={"nav-item" + (view === "lists" ? " active" : "")}
            onClick={() => { setView("lists"); setSelectedFolder(""); setSideOpen(false); }}>
            <Book size={18} strokeWidth={1.9}/> Catálogo <span className="nav-count">{collection?.works.length || 0}</span>
          </button>
        </nav>
        <nav className="nav-group acquisition-nav" aria-label="Aquisição">
          <span className="nav-caption">Aquisição</span>
          <button aria-current={view === "sources" ? "page" : undefined} className={"nav-item" + (view === "sources" ? " active" : "")}
            onClick={() => { setView("sources"); setSelectedFolder(""); setSideOpen(false); }}>
            <Globe2 size={18} strokeWidth={1.9}/> Descobrir
          </button>
          <button aria-current={view === "extensions" ? "page" : undefined} className={"nav-item" + (view === "extensions" ? " active" : "")}
            onClick={() => { setView("extensions"); setSelectedFolder(""); setSideOpen(false); }}>
            <Plug size={18} strokeWidth={1.9}/> Extensões
          </button>
          <button aria-current={view === "downloads" ? "page" : undefined} className={"nav-item" + (view === "downloads" ? " active" : "")}
            onClick={() => { setView("downloads"); setSelectedFolder(""); setSideOpen(false); }}>
            <ArrowDownToLine size={18} strokeWidth={1.9}/> Downloads
          </button>
        </nav>
        <div className="folders-header"><span className="nav-caption">PASTAS</span><button className="icon-button small" title="Criar pasta na pasta atual" onClick={() => openFolderDialog(selectedFolder)}><Plus size={18} /></button></div>
        <div className="folder-tree-search"><Search size={14}/><input aria-label="Filtrar pastas" placeholder="Localizar pasta…" value={folderQuery} onChange={(event) => setFolderQuery(event.target.value)}/>{folderQuery && <button aria-label="Limpar filtro de pastas" onClick={() => setFolderQuery("")}><X size={13}/></button>}</div>
        <div className="folder-scroller" role="tree" aria-label="Estrutura da biblioteca">
          <div className={"folder-row" + (view === "catalog" && !selectedFolder ? " active" : "") + (dragTarget === "/" ? " drop-target" : "")}
            onContextMenu={(event) => openContextMenu(event, "folder", "")}
            onDragOver={(event) => { if (event.dataTransfer.types.includes("text/acervo-book")) { event.preventDefault(); setDragTarget("/"); } }}
            onDragLeave={() => setDragTarget("")}
            onDrop={(event) => { event.preventDefault(); queueMove(dropPaths(event.dataTransfer), ""); }}>
            <button className="folder-select" onClick={() => { setView("catalog"); setSelectedFolder(""); setSideOpen(false); }}>
              <FolderTree size={17} /><span>Raiz da biblioteca</span>
            </button>
            <button className="folder-quick-add" title="Nova pasta na raiz" aria-label="Nova pasta na raiz" onClick={() => openFolderDialog("")}><Plus size={15}/></button>
          </div>
          {visibleFolders.map((folder) => (
            <div key={folder} role="treeitem" aria-expanded={folderParents.has(folder) ? expandedFolders.has(folder) : undefined}
              className={"folder-row" + (view === "catalog" && selectedFolder === folder ? " active" : "") + (dragTarget === folder ? " drop-target" : "")}
              style={{ paddingLeft: 7 + Math.min(6, folder.split("/").length - 1) * 13 }} title={folder}
              onContextMenu={(event) => openContextMenu(event, "folder", folder)}
              onDragOver={(event) => { if (event.dataTransfer.types.includes("text/acervo-book")) { event.preventDefault(); setDragTarget(folder); } }}
              onDragLeave={() => setDragTarget("")}
              onDrop={(event) => { event.preventDefault(); queueMove(dropPaths(event.dataTransfer), folder); }}>
              {folderParents.has(folder) ? <button className="folder-expander" aria-label={expandedFolders.has(folder) ? "Recolher pasta" : "Expandir pasta"} onClick={() => setExpandedFolders((prev) => {
                const next = new Set(prev); if (next.has(folder)) next.delete(folder); else next.add(folder); return next;
              })}>{expandedFolders.has(folder) ? <ChevronDown size={14}/> : <ChevronRight size={14}/>}</button> : <span className="folder-expander-spacer"/>}
              <button className="folder-select" onClick={() => { setSelectedFolder(folder); setView("catalog"); setSideOpen(false); if (folderParents.has(folder)) setExpandedFolders((prev) => new Set(prev).add(folder)); }}>
                <Folder size={16} strokeWidth={1.8}/><span>{folder.split("/").slice(-1)[0]}</span><small className="tree-count">{folderFileCounts.get(folder) || 0}</small>
              </button>
              <button className="folder-quick-add" title={"Criar subpasta em " + folder} aria-label={"Criar subpasta em " + folder} onClick={(event) => { event.stopPropagation(); openFolderDialog(folder); }}><Plus size={15}/></button>
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
          <div className="topbar-search"><Search size={18} /><input ref={searchRef} value={search} onChange={(event) => { setSearch(event.target.value); setActiveSuggestion(-1); }} onFocus={() => { if (view !== "catalog" && view !== "lists") { setView("catalog"); setSelectedFolder(""); } setSearchFocused(true); }} onBlur={() => setSearchFocused(false)}
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
            <button className="icon-button" title="Atualizar catálogo" aria-label="Atualizar catálogo" disabled={loading} onClick={() => void refresh()}><RefreshCw className={loading ? "spinning" : ""} size={19} /></button>
          </div>
        </header>
        <main ref={mainRef} className="main-area" data-view={view} onContextMenu={(event) => { if (view === "catalog") openContextMenu(event, "background", selectedFolder); }}>
          {view === "settings" ? <section className="settings-page">
            {error && <div className="alert" role="alert"><CircleHelp size={18}/><span>{error}</span><button onClick={() => setError("")} aria-label="Dispensar erro"><X size={16}/></button></div>}
            <div className="page-intro"><div><h1>Configurações</h1><p>Personalize a aparência e o armazenamento da biblioteca.</p></div></div>
            <div className="settings-grid">
              <article className="settings-card glass-panel">
                <div className="settings-card-heading"><Palette size={22}/><div><h2>Aparência</h2><p>Escolha o estilo visual.</p></div></div>
                <div className="style-choices" role="group" aria-label="Estilo da interface">
                  <button className={"style-choice " + (visualStyle === "workspace" ? "chosen" : "")} onClick={() => setVisualStyle("workspace")} aria-pressed={visualStyle === "workspace"}><span className="style-preview" data-preview="workspace" aria-hidden="true"><i/><b><em/><em/><em/></b></span><strong>Acervo</strong><small>Superfícies sólidas, capas em destaque</small></button>
                  <button className={"style-choice " + (visualStyle === "glass" ? "chosen" : "")} onClick={() => setVisualStyle("glass")} aria-pressed={visualStyle === "glass"}><span className="style-preview" data-preview="glass" aria-hidden="true"><i/><b><em/><em/><em/></b></span><strong>Liquid Glass</strong><small>Transparência, brilho e profundidade</small></button>
                  <button className={"style-choice " + (visualStyle === "material" ? "chosen" : "")} onClick={() => setVisualStyle("material")} aria-pressed={visualStyle === "material"}><span className="style-preview" data-preview="material" aria-hidden="true"><i/><b><em/><em/><em/></b></span><strong>Material 3</strong><small>Superfícies tonais · Niri + Noctalia</small></button>
                </div>
                <label className="settings-field">Paleta de cores
                  <select aria-label="Paleta de cores" value={palette} onChange={(event) => setPalette(event.target.value as PaletteName)}><option value="noctalia">Automática · Noctalia</option><option value="jade">Jade</option><option value="indigo">Índigo</option><option value="rose">Rosé</option><option value="amber">Âmbar</option></select>
                </label>
                <div className="theme-sync"><span className={"status-dot " + (theme ? "" : "muted")}/><div><strong>{theme ? "Noctalia conectado" : "Aguardando paleta do Noctalia"}</strong><small>{theme ? "As cores acompanham as alterações do wallpaper automaticamente." : "Para sincronizar, habilite o template do Acervo no Noctalia."}</small></div></div>
              </article>
              <article className="settings-card glass-panel">
                <div className="settings-card-heading"><HardDrive size={22}/><div><h2>Biblioteca</h2></div></div>
                <div className="current-root"><FolderOpen size={20}/><span>{collection?.root || "Nenhuma biblioteca selecionada"}</span></div>
                <div className="settings-actions"><button className="button primary" onClick={() => void chooseRoot()}><FolderOpen size={17}/> Procurar pasta</button><button className="button secondary" onClick={() => setModal("root")}>Informar caminho</button></div>
                <label className="settings-toggle"><span><strong>Monitorar alterações</strong><small>Atualiza a biblioteca quando arquivos ou pastas mudam.</small></span><input type="checkbox" role="switch" checked={monitorEnabled} onChange={event=>setMonitorEnabled(event.target.checked)}/></label>{monitorError&&<small className="download-error" role="alert">{monitorError}</small>}<p className="settings-footnote">Abrir uma biblioteca não altera arquivos. Movimentações exigem confirmação e não substituem arquivos existentes.</p>
              </article>
              <article className="settings-card glass-panel">
                <div className="settings-card-heading"><ArrowDownToLine size={22}/><div><h2>Aquisição</h2></div></div>
                <label className="settings-field">Nome de novos arquivos
                  <select value={namingPreset} onChange={event=>setNamingPreset(readNamingPreset(event.target.value))}>{namingOptions.map(option=><option key={option.id} value={option.id}>{option.label}</option>)}</select>
                </label>
                <div className="current-root"><Book size={19}/><span>{namingOptions.find(option=>option.id===namingPreset)?.example}</span></div>
                <p className="settings-footnote">Aplica-se a novos downloads. Arquivos existentes e itens da fila não são renomeados.</p>
              </article>
            </div>
            <div className="settings-version">Acervo 0.1.0-alpha.2 · dados locais · <span>atualizações experimentais</span></div>
          </section> : (view === "sources" || view === "extensions") ? <SourcesCenter namingPreset={namingPreset} root={collection?.root || ""} works={collection?.works || []} mode={view === "sources" ? "discover" : "extensions"} initialSearch={sourceRequest} onClearRequest={() => setSourceRequest(null)} onShowExtensions={() => setView("extensions")} onShowDiscover={() => setView("sources")} onShowDownloads={() => { setView("downloads"); setSelectedFolder(""); }}/> : view === "downloads" ? <DownloadsCenter root={collection?.root || ""} folders={collection?.folders || []} onSaved={async () => { await refresh(); }}/> : view === "lists" ? <>
            {error && <div className="alert" role="alert"><CircleHelp size={18}/><span>{error}</span><button onClick={() => setError("")} aria-label="Dispensar erro"><X size={16}/></button></div>}
            {status && <div className="inline-status" role="status"><Check size={16}/>{status}</div>}
            <CatalogCenter works={collection?.works || []} files={collection?.files || []} query={search}
              onImport={(prefill = "") => { setImportText(prefill); setImportNotice(""); setModal("import"); }}
              onPrompt={() => { setPromptDraft(null); setModal("prompt"); }}
              onExport={(json) => void copy(json, "Lista JSON copiada. Você pode salvar ou compartilhar o conteúdo.")}
              onExportCsv={(csv) => void exportCsv(csv)}
              onCatalogUnindexed={(json) => void importBooks(json)}
              onCatalogWithAI={(books) => void prepareAiCatalog(books)}
              onSelect={(work) => { const entry = entries.find((candidate) => candidate.work?.id === work.id); if (entry) setSelection(entry); }}
              onSelectFile={(file) => { const entry = libraryEntries.find((candidate) => candidate.file?.path === file.path); if (entry) setSelection(entry); }} />
          </> : <>
          <div className="page-intro"><div><h1>{currentLabel}</h1><p>{collection ? collection.files.length + " arquivos na biblioteca" : "Nenhuma biblioteca selecionada"}</p></div><div className="page-actions"><ActionMenu label="Gerenciar" icon={<Layers3 size={17}/>} actions={[
            {label:"Duplicatas",icon:<Archive size={17}/>,disabled:!collection||loading,run:()=>setDuplicateRoot(collection?.root??"")},
            {label:"Metadados",icon:<Book size={17}/>,disabled:!collection||loading,run:()=>setMetadataRoot(collection?.root??"")},
            {label:"Organizar por JSON",icon:<Layers3 size={17}/>,disabled:!collection||loading,run:openOrganizationDialog},
          ]}/><button className="button secondary" disabled={!collection||loading} onClick={()=>setSmartRoot(collection?.root??"")}><FolderTree size={17}/> Organizar</button><button className="button primary add-button" disabled={!collection||loading} onClick={() => openFolderDialog(selectedFolder)}><FolderPlus size={18}/> Nova pasta</button></div></div>
          {error && <div className="alert" role="alert"><CircleHelp size={18}/><span>{error}</span><button onClick={() => setError("")} aria-label="Dispensar erro"><X size={16}/></button></div>}
          {collection?.warnings.length ? <div className="warning">{collection.warnings.slice(0, 2).join(" · ")}</div> : null}
          {selectedFolder && <nav className="breadcrumbs" aria-label="Caminho da pasta"><button onClick={() => { setSelectedFolder(""); setView("catalog"); }}><Home size={15}/> Raiz</button>{breadcrumbs.map((crumb) => <span key={crumb.path}><ChevronRight size={14}/><button onClick={() => setSelectedFolder(crumb.path)}>{crumb.label}</button></span>)}</nav>}
          {subfolders.length > 0 && <section className="folder-grid" aria-label="Subpastas">{subfolders.map((folder) => (
            <button className="folder-tile glass-panel" key={folder} onContextMenu={(event) => openContextMenu(event, "folder", folder)} onClick={() => { setSelectedFolder(folder); setView("catalog"); setExpandedFolders((current) => new Set(current).add(folder)); }}>
              <Folder size={23}/><span><strong>{folder.split("/").slice(-1)[0]}</strong><small>{folderFileCounts.get(folder) || 0} livros dentro</small></span><ChevronRight size={17}/>
            </button>
          ))}</section>}
          <div className="content-top"><div><h2>Arquivos <span>{filtered.length}</span></h2></div><div className="collection-controls"><button className={"selection-toggle" + (selectionMode ? " active" : "")} onClick={() => { setSelectionMode(!selectionMode); setBulkSelected(new Set()); selectionAnchor.current = null; }}>{selectionMode ? <Check size={16}/> : <List size={16}/>} {selectionMode ? "Concluir" : "Selecionar"}</button><div className="pill-control"><Filter size={16}/><select aria-label="Filtrar formato" value={format} onChange={(event) => setFormat(event.target.value)}>{formatOptions.map((item) => <option key={item}>{item}</option>)}</select><ChevronDown size={14}/></div><div className="pill-control sort-control"><ArrowUpDown size={16}/><select aria-label="Ordenar livros" value={sortBy} onChange={(event) => setSortBy(event.target.value as SortBy)}><option value="title">Título</option><option value="author">Autor</option><option value="format">Formato</option><option value="size">Tamanho</option></select><ChevronDown size={14}/></div><div className="pill-control group-control"><Layers3 size={15}/><select aria-label="Agrupar livros" value={groupBy} onChange={(event) => setGroupBy(event.target.value as GroupBy)}><option value="none">Sem grupos</option><option value="folder">Por pasta</option><option value="format">Por formato</option><option value="author">Por autor</option></select><ChevronDown size={14}/></div><div className="view-switch" role="group" aria-label="Modo de visualização"><button className={layout === "grid" ? "chosen" : ""} title="Grade" aria-label="Grade" aria-pressed={layout === "grid"} onClick={() => setLayout("grid")}><Grid2X2 size={17}/></button><button className={layout === "list" ? "chosen" : ""} title="Lista" aria-label="Lista" aria-pressed={layout === "list"} onClick={() => setLayout("list")}><List size={17}/></button></div></div></div>
          {bulkSelected.size > 0 && <div className="bulk-toolbar glass-panel"><div><Check size={17}/><strong>{bulkSelected.size} selecionado(s)</strong><small>Arraste para uma pasta ou escolha um destino.</small></div><button className="button primary" onClick={() => queueMove([...bulkSelected], selectedFolder || "")}><FolderOpen size={17}/> Mover para…</button><button className="icon-button" onClick={() => { setBulkSelected(new Set()); selectionAnchor.current = null; }} title="Limpar seleção" aria-label="Limpar seleção"><X size={17}/></button></div>}
          {!collection ? <div className="empty-state glass-panel"><Archive size={42}/><h3>Seu acervo, no seu controle.</h3><p>Selecione uma pasta para visualizar os livros e o catálogo existente. Nada será alterado ao abrir.</p><button className="button primary" onClick={() => void chooseRoot()}>Escolher pelo explorador <FolderOpen size={16}/></button></div>
          : filtered.length ? virtualList ? <VirtualBookList key={JSON.stringify([collection.root,selectedFolder,format,search,sortBy])} items={displayEntries} scrollRef={mainRef} render={renderBookCard}/> : <><div className={"books-" + layout}>{visibleEntries.map((entry, index) => (
            <Fragment key={entry.key}>
              {groupBy !== "none" && (index === 0 || groupLabel(entry) !== groupLabel(visibleEntries[index - 1])) &&
                <div className="book-group-heading"><span>{groupLabel(entry)}</span><span className="book-group-rule"/></div>}
            {renderBookCard(entry)}
            </Fragment>
          ))}</div>{visibleLimit < filtered.length && <button className="load-more" onClick={() => setVisibleLimit((current) => current + 80)}>Mostrar mais livros <span>{visibleEntries.length} de {filtered.length}</span><ChevronDown size={16}/></button>}</> : <div className="empty-state glass-panel"><Search size={35}/><h3>Nenhum arquivo nesta pasta</h3><p>Escolha outra pasta ou altere os filtros. Obras faltantes aparecem exclusivamente no Catálogo.</p></div>}
          {status && <div className="inline-status" role="status"><Check size={16}/>{status}</div>}
          </>}
        </main>
      </div>
      {selection && <div className="drawer-backdrop" onClick={() => setSelection(null)}><aside aria-label="Detalhes da obra" className="details-drawer glass-panel" onClick={(event) => event.stopPropagation()}><div className="drawer-head"><span className="eyebrow">DETALHES DA OBRA</span><button className="icon-button" onClick={() => setSelection(null)} aria-label="Fechar detalhes"><X size={19}/></button></div><div className="details-cover"><BookCover key={selection.key} root={collection?.root || ""} file={selection.file} format={selection.format} present={selection.present} title={selection.title}/></div><h2>{selection.title}</h2><p className="drawer-author">{selection.author || "Sem autor cadastrado"}</p><div className="detail-list"><div><span>Situação</span><strong>{selection.present ? "Disponível" : "Na lista de desejos"}</strong></div><div><span>Formato</span><strong>{selection.format}</strong></div><div><span>Pasta planejada</span><strong>{selection.folder || "Raiz"}</strong></div>{selection.file && <div><span>Arquivo</span><strong>{selection.file.path}</strong></div>}{selection.file && <div><span>Tamanho</span><strong>{readableBytes(selection.file.bytes)}</strong></div>}</div>{selection.file&&collection&&<section className="edition-options"><div className="smart-review-head"><h3>Metadados manuais</h3><button className="button secondary" onClick={()=>setManualEdit({root:collection.root,file:selection.file!})}>Editar metadados</button></div>{selection.file.manual&&<div className="detail-list">{metadataFields.filter(([key])=>selection.file?.manual?.metadata[key]).map(([key,label])=><div key={key}><span>{label}</span><strong>{selection.file?.manual?.metadata[key]}</strong></div>)}</div>}</section>}{selection.file?.edition && <section className="edition-options"><h3>Edição adquirida</h3><div className="detail-list">
        {([["Título original",selection.file.edition.metadata.title],["Autor informado",selection.file.edition.metadata.author],["Idioma",selection.file.edition.metadata.language],["Editora",selection.file.edition.metadata.publisher],["Edição",selection.file.edition.metadata.editionLabel],["Fonte",selection.file.edition.sourceName]] as const).filter(([,value])=>value).map(([label,value])=><div key={label}><span>{label}</span><strong>{value}</strong></div>)}
        </div><small>Metadados informados pela fonte.</small></section>}{selection.file?.embedded?.metadata && <section className="edition-options"><h3>Metadados do arquivo</h3><div className="detail-list">{([["Título",selection.file.embedded.metadata.title],["Autor",selection.file.embedded.metadata.author],["Idioma",selection.file.embedded.metadata.language],["Editora",selection.file.embedded.metadata.publisher],["ISBN",selection.file.embedded.metadata.isbn],["Descrição",selection.file.embedded.metadata.description]] as const).filter(([,value])=>value).map(([label,value])=><div key={label}><span>{label}</span><strong>{value}</strong></div>)}</div></section>}{selection.file?.embedded?.error&&<section className="edition-options"><h3>Metadados do arquivo</h3><small>{selection.file.embedded.error}</small></section>}{selection.work && selection.work.id < 0 && <section className="edition-options"><h3>Arquivos associados</h3>
        {selection.work.associatedFiles.length > 0 && <div className="associated-paths">{selection.work.associatedFiles.map(path => {
          const file=collection?.files.find(file=>file.path===path);
          const label=[file?.edition?.metadata.editionLabel,file?.edition?.metadata.publisher,file?.edition?.metadata.language].filter(Boolean).join(" · ");
          return <span key={path} title={path}>{path}{label && <small className="associated-edition-label">{label}</small>}</span>;
        })}</div>}
        <button className="button secondary full" disabled={associationBusy} onClick={() => void associateExistingFile(selection.work!)}><Link2 size={16}/> Associar arquivo existente</button>
        {associationError && <p className="association-error" role="alert">{associationError}</p>}
      </section>}{selection.work && selectedAlternatives.length > 0 && <section className="edition-options"><h3>{selection.present ? "Outros formatos no acervo" : "Possível edição já disponível"}</h3><p>Encontramos arquivos com o mesmo título. Confira a obra e a edição antes de associar ao catálogo.</p>{selectedAlternatives.slice(0, 8).map((file) => <div key={file.path} className="edition-option-row"><button className="edition-option" title={file.path} onClick={() => { const entry = libraryEntries.find((candidate) => candidate.file?.path === file.path); if (entry) setSelection(entry); }}><Book size={16}/><span><strong>{file.format}</strong><small>{file.path}</small></span><ArrowRight size={15}/></button>{selection.work!.id < 0 && <button className="icon-button" disabled={associationBusy} aria-label={"Associar " + file.path} title="Associar edição" onClick={() => void associateExistingFile(selection.work!, file.path)}><Link2 size={16}/></button>}</div>)}</section>}{!selection.present && <div className="search-source"><h3>Buscar edição</h3><p>Pesquise nas fontes instaladas. Confira a origem e os direitos de uso antes de baixar.</p><label>Formato <select value={sourceFormat} onChange={(event) => setSourceFormat(event.target.value)}><option value="">Qualquer formato</option><option value="epub">EPUB</option><option value="pdf">PDF</option><option value="mobi">MOBI</option></select></label><label>Idioma <select value={sourceLanguage} onChange={(event) => setSourceLanguage(event.target.value)}>{languageOptions.map((item) => <option key={item.value} value={item.value}>{item.label}</option>)}</select></label><button className="button primary full" onClick={() => searchInInstalledSources(selection)}><Search size={17}/> Pesquisar nas fontes</button></div>}<button className="button secondary full" onClick={() => void copy(selection.folder || ".", "Destino copiado para a área de transferência.")}><ClipboardCopy size={17}/> Copiar pasta de destino</button></aside></div>}
      {manualEdit&&collection?.root===manualEdit.root&&<ManualMetadataEditor key={manualEdit.root+":"+manualEdit.file.path} root={manualEdit.root} file={manualEdit.file} onClose={()=>setManualEdit(null)} onChanged={async()=>{if(requestedRoot.current===manualEdit.root){await refresh(manualEdit.root);setSelection(null);}}}/> }
      {duplicateRoot && collection?.root===duplicateRoot && <DuplicatesCenter key={duplicateRoot} root={duplicateRoot} folders={collection.folders} initialFolder={selectedFolder} onClose={()=>setDuplicateRoot("")} onInspect={path=>{const entry=libraryEntries.find(item=>item.file?.path===path);if(entry){setDuplicateRoot("");setSelection(entry);}else{setError("Arquivo não encontrado; atualize a biblioteca.");setDuplicateRoot("");}}}/> }
      {metadataRoot && collection?.root===metadataRoot && <MetadataCenter key={metadataRoot} root={metadataRoot} files={collection.files} cacheWarning={collection.warnings.find(warning=>warning.startsWith("Cache de metadados:"))} onClose={()=>setMetadataRoot("")} onChanged={async()=>{if(requestedRoot.current===metadataRoot){await refresh(metadataRoot);setSelection(null);}}}/> }
      {smartRoot && collection?.root===smartRoot && <SmartOrganization key={smartRoot} root={smartRoot} folders={collection.folders} onClose={()=>setSmartRoot("")} onChanged={async()=>{await refresh(smartRoot);setSelection(null);setBulkSelected(new Set());}}/>}
      {modal && <div className="modal-backdrop" onMouseDown={(event) => { if (event.target === event.currentTarget) setModal(null); }}><section className={"modal glass-panel" + (modal === "organize" ? " organize-modal" : "")} role="dialog" aria-modal="true"><div className="modal-header"><div><span className="eyebrow">ACERVO</span><h2>{modal === "root" ? "Selecionar biblioteca" : modal === "prompt" ? "Gerador de listas" : modal === "folder" ? "Nova pasta" : modal === "move" ? "Mover livros" : modal === "organize" ? "Organizar biblioteca por JSON" : "Importar lista JSON"}</h2></div><button className="icon-button" onClick={() => setModal(null)} aria-label="Fechar"><X size={20}/></button></div>
        {modal === "root" && <div className="modal-body"><p>Escolha a pasta raiz no explorador. O acervo será lido sem alterar livros ou pastas.</p><button className="button primary full" disabled={loading} onClick={() => void chooseRoot()}>{loading ? <LoaderCircle className="spinning" size={18}/> : <FolderOpen size={18}/>} Escolher pelo explorador</button><div className="manual-path"><label>Ou informe o caminho manualmente<input value={root} onChange={(event) => setRoot(event.target.value)} placeholder="/home/usuario/Books" /></label><button className="button secondary full" disabled={loading || !root.trim()} onClick={() => void refresh(root)}>Carregar caminho informado</button></div></div>}
        {modal === "prompt" && <div className="modal-body"><p>O prompt utiliza as pastas reais da sua biblioteca e produz um JSON compatível com o catálogo antigo.</p><div className="two-inputs"><label>Quantidade<input type="number" min={1} max={150} value={promptCount} onChange={(event) => setPromptCount(Math.min(150, Math.max(1, Number(event.target.value) || 1)))} /></label><label>Idioma<input value={promptLanguage} onChange={(event) => setPromptLanguage(event.target.value)} /></label></div><label>Temas e critérios<textarea value={promptTopics} onChange={(event) => setPromptTopics(event.target.value)} rows={2}/></label>
          <PromptFolderPicker folders={promptDirectoryOptions} selected={promptFolders} onChange={(next) => { setPromptFolders(next); setPromptDraft(null); }} />
          <label>Prévia do prompt (editável)<textarea value={promptDraft ?? createPrompt()} onChange={(event) => setPromptDraft(event.target.value)} rows={11} spellCheck={false}/></label>
          {promptDraft !== null && <button className="button secondary full" onClick={() => setPromptDraft(null)}>Restaurar texto sugerido</button>}
          <button className="button primary full" onClick={() => void copy(promptDraft ?? createPrompt(), "Prompt copiado. Cole na IA de sua preferência.")}><ClipboardCopy size={17}/> Copiar prompt</button></div>}
        {modal === "import" && <div className="modal-body"><p>Cole uma lista JSON no formato <code>{'{ "versao": 1, "obras": [...] }'}</code>. As obras serão salvas em <code>99 - Catálogo/Acervo_Listas.json</code>; o banco antigo não será alterado.</p><label>Conteúdo JSON<textarea value={importText} onChange={(event) => { setImportText(event.target.value); setImportNotice(""); }} rows={11} placeholder='{"versao":1,"obras":[{"titulo":"O Banquete","local_indicado":"Literatura/Filosofia"}]}' /></label>{importNotice && <div className="import-notice">{importNotice}</div>}<button className="button primary full" disabled={loading || !collection || !importText.trim()} onClick={() => void importBooks()}>{loading ? <LoaderCircle size={17} className="spinning"/> : <Check size={17}/>} Importar obras</button></div>}
        {modal === "organize" && <div className="modal-body organization-body">
          <p>Envie à IA apenas o inventário de caminhos relativos. Confira a proposta antes de mover qualquer livro. O Acervo não envia seus dados automaticamente.</p>
          <button className="button secondary full" disabled={!collection} onClick={() => void copy(createOrganizationPrompt(), "Inventário e instruções copiados para a IA.")}>
            <ClipboardCopy size={17}/> Copiar inventário e prompt para IA
          </button>
          <label>Cole o plano retornado pela IA (JSON)
            <textarea spellCheck={false} rows={8} value={organizationText}
              placeholder='{"versao":1,"movimentacoes":[{"arquivo":"00 - Entrada/Livro.epub","destino":"Literatura","tamanho_bytes":12345}]}'
              onChange={(event) => { setOrganizationText(event.target.value); setOrganizationPreview(null); setOrganizationConfirmed(false); setOrganizationError(""); }}/>
          </label>
          <button className="button primary full" disabled={!collection || !organizationText.trim() || loading} onClick={() => void previewOrganization()}>
            <Search size={17}/> Conferir movimentações (sem alterar arquivos)
          </button>
          {organizationError && <div className="warning" role="alert">{organizationError}</div>}
          {organizationPreview && <div className="organization-review">
            <strong>Prévia · {organizationPreview.items.length} livro(s), {readableBytes(organizationPreview.totalBytes)}</strong>
            <div className="organization-moves">
              {organizationPreview.items.map((item) => <div className="organization-move" key={item.source}>
                <span title={item.source}>{item.source}</span><ArrowRight size={15}/><strong title={item.target}>{item.target}</strong>
              </div>)}
            </div>
            <label className="organization-approval"><input type="checkbox" checked={organizationConfirmed}
              onChange={(event) => setOrganizationConfirmed(event.target.checked)}/>
              Conferi os destinos. Autorizo mover estes arquivos da biblioteca.
            </label>
            <button className="button primary full" disabled={!organizationConfirmed || loading} onClick={() => void applyOrganization()}>
              {loading ? <LoaderCircle className="spinning" size={17}/> : <Check size={17}/>}
              Aplicar {organizationPreview.items.length} movimentação(ões)
            </button>
          </div>}
        </div>}
        {modal === "folder" && <div className="modal-body"><p>A nova pasta será criada em <strong>{folderParent || "Raiz da biblioteca"}</strong>.</p><label>Nome da pasta<input autoFocus value={newFolder} onChange={(event) => setNewFolder(event.target.value)} onKeyDown={(event) => { if (event.key === "Enter" && newFolder.trim()) void addFolder(); }} placeholder="Ex.: Filosofia" /></label><button className="button primary full" disabled={!collection || !newFolder.trim()} onClick={() => void addFolder()}><FolderPlus size={17}/> Criar pasta</button></div>}
        {modal === "move" && pendingMove && <div className="modal-body">
          <p>Revise a movimentação. Arquivos com o mesmo nome no destino não serão sobrescritos, e as referências no catálogo serão atualizadas.</p>
          <div className="move-summary"><strong>{pendingMove.paths.length} livro(s) selecionado(s)</strong><span>{pendingMove.paths.slice(0, 5).map((path) => <small key={path}>{path}</small>)}{pendingMove.paths.length > 5 && <small>e mais {pendingMove.paths.length - 5}…</small>}</span></div>
          <label>Pasta de destino<select value={pendingMove.destination} onChange={(event) => setPendingMove({ ...pendingMove, destination: event.target.value })}><option value="">Raiz da biblioteca</option>{collection?.folders.map((folder) => <option key={folder} value={folder}>{folder}</option>)}</select></label>
          {moveConflicts.length > 0 && <div className="warning">Há {moveConflicts.length} conflito(s): arquivo já presente no destino, nome repetido ou livro já nesta pasta. Selecione outro destino.</div>}
          {error && <div className="alert" role="alert">{error}</div>}
          <div className="move-actions"><button className="button secondary" onClick={() => setModal(null)}>Cancelar</button><button className="button primary" disabled={loading || moveConflicts.length > 0} onClick={() => void commitMove()}>{loading ? <LoaderCircle size={17} className="spinning"/> : <FolderOpen size={17}/>} Confirmar movimentação</button></div>
        </div>}
      </section></div>}
      {contextMenu && <ContextMenu x={contextMenu.x} y={contextMenu.y} title={contextMenu.kind === "file" ? contextMenu.path.split("/").slice(-1)[0] : contextMenu.path || "Raiz da biblioteca"} actions={contextActions} onClose={() => setContextMenu(null)} />}
    </div>
  );
}

export default App;
