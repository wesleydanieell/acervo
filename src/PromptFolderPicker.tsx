import { useMemo, useState } from "react";
import { Check, ChevronRight, Folder, FolderOpen, Search, X } from "lucide-react";
import {
  chosenPromptFolders, directPromptChildren, promptFolderSelection, promptSubtree,
  togglePromptFolder, type FolderSelection,
} from "./prompt-folder-selection";

type Props = {
  folders: string[];
  selected: ReadonlySet<string> | null;
  onChange: (folders: Set<string> | null) => void;
};
const fold = (value: string) => value.normalize("NFD").replace(/[\u0300-\u036f]/g, "").toLowerCase();
const labelFor = (path: string) => path.split("/").slice(-1)[0] || "Biblioteca";

function FolderToggle({ state, onToggle, label }: {
  state: FolderSelection;
  onToggle: () => void;
  label: string;
}) {
  return <input type="checkbox" checked={state === "all"}
    aria-label={label} aria-checked={state === "partial" ? "mixed" : state === "all"}
    ref={(element) => { if (element) element.indeterminate = state === "partial"; }}
    onChange={onToggle}/>;
}

export function PromptFolderPicker({ folders, selected, onChange }: Props) {
  const [current, setCurrent] = useState("");
  const [query, setQuery] = useState("");
  const chosen = chosenPromptFolders(folders, selected);
  const isSearching = Boolean(query.trim());
  const entries = useMemo(() => isSearching
    ? folders.filter((folder) => fold(folder).includes(fold(query.trim())))
    : directPromptChildren(folders, current),
  [folders, current, query, isSearching]);
  const shown = entries.slice(0, 100);
  const parts = current.split("/").filter(Boolean).map((part, index, all) => ({
    name: part, path: all.slice(0, index + 1).join("/"),
  }));
  const currentSelection = promptFolderSelection(folders, selected, current);

  return <section className="prompt-folder-picker" aria-label="Pastas para o prompt">
    <header className="prompt-folder-head">
      <strong>Destinos da lista</strong>
      <span>{chosen.length} de {folders.length} pastas</span>
    </header>
    <p>Navegue como em um explorador. Marcar uma pasta inclui automaticamente todas as subpastas.</p>
    <div className="prompt-folder-controls">
      <div className="prompt-folder-search"><Search size={15} aria-hidden="true"/><input
        aria-label="Pesquisar pastas" value={query} onChange={(event) => setQuery(event.target.value)}
        placeholder="Procurar pasta…" />
        {query && <button className="prompt-folder-clear" aria-label="Limpar busca" onClick={() => setQuery("")}><X size={14}/></button>}
      </div>
      <button className="button secondary" type="button" onClick={() => onChange(null)}>Toda biblioteca</button>
      <button className="button secondary" type="button" onClick={() => onChange(new Set())}>Só raiz</button>
    </div>
    <nav className="prompt-folder-breadcrumbs" aria-label="Caminho atual">
      <button type="button" className={!current ? "current" : ""} onClick={() => { setCurrent(""); setQuery(""); }}>
        <FolderOpen size={14}/> Biblioteca
      </button>
      {parts.map((part) => <span key={part.path}>
        <ChevronRight size={13} aria-hidden="true"/>
        <button type="button" className={part.path === current ? "current" : ""}
          title={part.path} onClick={() => { setCurrent(part.path); setQuery(""); }}>{part.name}</button>
      </span>)}
    </nav>
    <div className="prompt-folder-list" role="group" aria-label="Navegação e seleção de diretórios">
      {!isSearching && <div className="prompt-folder-row current-folder">
        <label className="prompt-folder-select" title={current || "Toda a biblioteca"}>
          <FolderToggle state={currentSelection} onToggle={() => onChange(togglePromptFolder(folders, selected, current))}
            label={current ? "Selecionar esta pasta e subpastas" : "Selecionar toda a biblioteca"}/>
        </label>
        <div className="prompt-folder-current-title"><FolderOpen size={15}/>
          <strong>{current ? labelFor(current) + " (pasta atual)" : "Biblioteca inteira"}</strong>
          <small>{current ? promptSubtree(folders, current).length + " pasta(s) neste ramo" : "Todas as pastas e subpastas"}</small>
        </div>
        {currentSelection === "all" && <Check size={15} aria-hidden="true"/>}
      </div>}
      {shown.map((folder) => {
        const state = promptFolderSelection(folders, selected, folder);
        const descendants = promptSubtree(folders, folder).length - 1;
        return <div className={"prompt-folder-row" + (state !== "none" ? " active" : "")} key={folder}>
          <label className="prompt-folder-select" title="Marcar este ramo inteiro">
            <FolderToggle state={state}
              onToggle={() => onChange(togglePromptFolder(folders, selected, folder))}
              label={"Selecionar " + folder + " e suas subpastas"}/>
          </label>
          <button type="button" className="prompt-folder-open" title={folder}
            onClick={() => { setCurrent(folder); setQuery(""); }}>
            <Folder size={15} aria-hidden="true"/>
            <span><strong>{isSearching ? folder : labelFor(folder)}</strong>
              <small>{descendants ? descendants + " subpasta(s)" : "Sem subpastas"}</small></span>
            <ChevronRight size={16} aria-hidden="true"/>
          </button>
        </div>;
      })}
      {!shown.length && <div className="prompt-folder-empty"><Folder size={15}/>
        {isSearching ? "Nenhuma pasta corresponde à busca." : "Esta pasta não possui subpastas."}
      </div>}
    </div>
    {entries.length > shown.length && <small className="prompt-folder-hint">Mostrando 100 de {entries.length} nesta visão. Use a busca para encontrar outras. Selecionar o ramo inclui todas, mesmo as não exibidas.</small>}
    {!chosen.length && <small className="prompt-folder-hint">Somente a raiz será sugerida como destino.</small>}
  </section>;
}
