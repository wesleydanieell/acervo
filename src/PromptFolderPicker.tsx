import { useMemo, useState } from "react";
import { Check, Folder, Search, X } from "lucide-react";
import { chosenPromptFolders, togglePromptFolder } from "./prompt-folder-selection";

type Props = {
  folders: string[];
  selected: ReadonlySet<string> | null;
  onChange: (folders: Set<string> | null) => void;
};
const fold = (value: string) => value.normalize("NFD").replace(/[\u0300-\u036f]/g, "").toLowerCase();

export function PromptFolderPicker({ folders, selected, onChange }: Props) {
  const [query, setQuery] = useState("");
  const matches = useMemo(() => folders.filter((folder) => fold(folder).includes(fold(query.trim()))), [folders, query]);
  const chosen = chosenPromptFolders(folders, selected);
  const shown = matches.slice(0, 100);

  return <section className="prompt-folder-picker" aria-label="Pastas para o prompt">
    <header className="prompt-folder-head">
      <strong>Pastas disponíveis para a IA</strong>
      <span>{chosen.length} de {folders.length}</span>
    </header>
    <p>Marque somente os destinos que a IA poderá sugerir. Nenhum arquivo será movido.</p>
    <div className="prompt-folder-controls">
      <div className="prompt-folder-search"><Search size={15} aria-hidden="true"/><input
        aria-label="Filtrar pastas" value={query} onChange={(event) => setQuery(event.target.value)}
        placeholder="Filtrar pastas…" /></div>
      <button className="button secondary" type="button" onClick={() => onChange(null)}>Todas</button>
      <button className="button secondary" type="button" onClick={() => onChange(new Set())}>Nenhuma</button>
    </div>
    <div className="prompt-folder-list" role="group" aria-label="Selecionar diretórios">
      {shown.length ? shown.map((folder) => {
        const active = selected === null || selected.has(folder);
        return <label key={folder} className={"prompt-folder-option" + (active ? " active" : "")}>
          <input type="checkbox" checked={active}
            onChange={() => onChange(togglePromptFolder(folders, selected, folder))}/>
          <Folder size={14} aria-hidden="true"/><span title={folder}>{folder}</span>
          {active && <Check size={13} aria-hidden="true"/>}
        </label>;
      }) : <div className="prompt-folder-empty"><X size={15}/> Nenhuma pasta corresponde ao filtro.</div>}
    </div>
    {matches.length > shown.length && <small className="prompt-folder-hint">Mostrando 100 de {matches.length} pastas. Refine o filtro para encontrar outras.</small>}
    {!chosen.length && <small className="prompt-folder-hint">Sem destinos selecionados: o prompt usará apenas a raiz da biblioteca.</small>}
  </section>;
}
