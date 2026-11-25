import {Check,FolderOpen,ListChecks,Sparkles,X} from "lucide-react";
import {ActionMenu} from "./ActionMenu";
import {MotionPresence} from "./MotionPresence";
export function BulkSelectionBar({active,total,inResults,outside,results,busy,onAll,onInvert,onKeepResults,onClear,onMove,onAi}:{active:boolean;total:number;inResults:number;outside:number;results:number;busy:boolean;onAll:()=>void;onInvert:()=>void;onKeepResults:()=>void;onClear:()=>void;onMove:()=>void;onAi:()=>void}){
 return <MotionPresence>{active&&<section className="bulk-toolbar glass-panel" aria-label="Seleção em lote">
  <div className="batch-summary" role="status"><Check size={18}/><div><strong>{total} {total===1?"livro selecionado":"livros selecionados"}</strong><small>{inResults} de {results} nos resultados{outside>0?" · "+outside+" fora do filtro":""}</small></div></div>
  <div className="batch-actions">
    <ActionMenu label="Seleção" icon={<ListChecks size={16}/>} actions={[
      {label:"Selecionar todos os resultados",disabled:busy||!results||inResults===results,run:onAll},
      {label:"Inverter seleção nos resultados",disabled:busy||!results,run:onInvert},
      {label:"Manter somente os resultados",disabled:busy||!outside,run:onKeepResults},
      {label:"Limpar seleção",disabled:busy||!total,run:onClear},
    ]}/>
    <button type="button" className="button secondary" disabled={busy||!total} onClick={onAi}><Sparkles size={16}/> Organizar com IA</button>
    <button type="button" className="button primary" disabled={busy||!total} onClick={onMove}><FolderOpen size={16}/>{total>100?"Mover próximos 100…":"Mover para…"}</button>
    <button type="button" className="icon-button" disabled={busy||!total} aria-label="Limpar seleção" title="Limpar seleção" onClick={onClear}><X size={17}/></button>
  </div>
 </section>}</MotionPresence>;
}
