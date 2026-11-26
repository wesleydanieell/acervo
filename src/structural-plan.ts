import {isServiceCatalogPath} from "./prompt-folder-selection.ts";
export type StructuralOperation={kind:"createFolder";path:string}|{kind:"moveFolder";source:string;target:string};
export type PathChange={before:string;after:string};
export type StructuralPreview={
  version:number;filesystemRevision:string;planRevision:string|null;executionSupported:boolean;
  createdFolders:string[];movedFolders:{source:string;target:string;folders:number;files:number;books:number}[];
  referenceImpact:{works:{workId:number;title:string;associatedFiles:PathChange[];plannedDestination:PathChange|null}[];manualMetadata:PathChange[];editions:PathChange[];blockers:{path:string;reason:string}[];editionVerification:{checked:number;deferred:number;bytesReserved:number}};
  downloadImpact:{changes:{id:string;title:string;status:string;plannedDestination:PathChange|null;catalogDestination:PathChange|null}[];blockers:{id:string;title:string;reason:string}[];destinationConflicts:{target:string;downloadIds:string[]}[]}|null;
  ruleImpact:{ruleName:string;enabled:boolean;destination:PathChange}[]|null;
};
export const folderParent=(path:string)=>path.split("/").slice(0,-1).join("/");
export const structuralPath=(path:string)=>path!==""&&new TextEncoder().encode(path).length<=600&&path.split("/").length<=80&&path.split("/").every(part=>!isServiceCatalogPath(part)&&part!==""&&!part.startsWith(".")&&part!=="node_modules"&&part.trim()===part&&!/[\\:\x00-\x1f\x7f]/.test(part)&&new TextEncoder().encode(part).length<=255);
const within=(path:string,parent:string)=>path===parent||path.startsWith(parent+"/");
export function draftOperation(kind:StructuralOperation["kind"],source:string,parent:string,name:string):StructuralOperation{
  if(name.includes("/")||!structuralPath(name)||parent!==""&&!structuralPath(parent))throw new Error("Informe um nome de pasta válido.");
  const target=parent?parent+"/"+name:name;
  if(!structuralPath(target))throw new Error("Caminho de destino excede os limites.");
  if(kind==="createFolder")return {kind,path:target};
  if(!structuralPath(source)||within(target,source)||within(source,target))throw new Error("Origem e destino não podem se sobrepor.");
  return {kind,source,target};
}
export function projectFolders(folders:readonly string[],operations:readonly StructuralOperation[]):string[]{
  const current=folders.filter(structuralPath);
  const moves=operations.filter((op):op is Extract<StructuralOperation,{kind:"moveFolder"}>=>op.kind==="moveFolder");
  const projected=current.map(path=>{
    const move=moves.find(op=>within(path,op.source));
    return move?move.target+path.slice(move.source.length):path;
  });
  for(const op of operations){if(op.kind==="createFolder")projected.push(op.path);}
  return [...new Set(projected)].sort((a,b)=>a.localeCompare(b,"pt-BR"));
}
export function draftParents(folders:readonly string[],operations:readonly StructuralOperation[],source:string):string[]{
  const moved=operations.filter((op):op is Extract<StructuralOperation,{kind:"moveFolder"}>=>op.kind==="moveFolder");
  return [...new Set([...folders.filter(structuralPath),...operations.flatMap(op=>op.kind==="createFolder"?[op.path]:[])])]
    .filter(path=>!moved.some(op=>within(path,op.source)||within(path,op.target))&&(!source||!within(path,source)))
    .sort((a,b)=>a.localeCompare(b,"pt-BR"));
}
export function impactCounts(preview:StructuralPreview){
  return {books:preview.movedFolders.reduce((n,folder)=>n+folder.books,0),works:preview.referenceImpact.works.length,blockers:preview.referenceImpact.blockers.length+(preview.downloadImpact?.blockers.length??0)+(preview.downloadImpact?.destinationConflicts.length??0)};
}
