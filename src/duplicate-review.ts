export type DuplicateFile={
  path:string;format:string;associations:number;manualMetadata:boolean;editionMetadata:boolean;protectionUnknown:boolean;
};
export type DuplicateGroup={bytes:number;sha256:string;files:DuplicateFile[]};
export type ReviewFilter="all"|"reviewable"|"protected";
export type ReviewSort="volume"|"copies"|"name";
export const fileName=(path:string)=>path.slice(path.lastIndexOf("/")+1);
export const folderName=(path:string)=>path.split("/").slice(0,-1).join("/")||"Raiz";
export function protections(file:DuplicateFile):string[]{
  const reasons:string[]=[];
  if(file.associations)reasons.push(file.associations+" vínculo(s) no catálogo");
  if(file.manualMetadata)reasons.push("Metadados manuais");
  if(file.editionMetadata)reasons.push("Registro de edição");
  if(file.protectionUnknown)reasons.push("Proteção não verificada");
  return reasons;
}
export const isProtected=(file:DuplicateFile)=>protections(file).length>0;
export function suggestedKeeper(group:DuplicateGroup):string{
  return [...group.files].sort((a,b)=>Number(isProtected(b))-Number(isProtected(a))||b.associations-a.associations||Number(b.manualMetadata)-Number(a.manualMetadata)||Number(b.editionMetadata)-Number(a.editionMetadata)||a.path.localeCompare(b.path,"pt-BR"))[0]?.path??"";
}
export function reviewGroup(group:DuplicateGroup,choice?:string){
  const keeper=group.files.find(file=>file.path===choice)?.path??suggestedKeeper(group);
  const remaining=group.files.filter(file=>file.path!==keeper);
  const reviewable=remaining.filter(file=>!isProtected(file));
  const blocked=remaining.filter(isProtected);
  return {keeper,reviewable,blocked,logicalBytes:reviewable.length*group.bytes};
}
const fold=(text:string)=>text.normalize("NFD").replace(/\p{Diacritic}/gu,"").toLowerCase();
export function filterGroups(groups:DuplicateGroup[],query:string,filter:ReviewFilter,sort:ReviewSort,choices:Record<string,string>){
  const needle=fold(query.trim());
  return groups.filter(group=>{
    const plan=reviewGroup(group,choices[group.sha256]);
    return (!needle||group.files.some(file=>fold(file.path+" "+file.format).includes(needle))||group.sha256.includes(needle))&&
      (filter==="all"||(filter==="protected"?group.files.some(isProtected):plan.reviewable.length>0));
  }).sort((a,b)=>{
    const volume=(g:DuplicateGroup)=>g.bytes*(g.files.length-1);
    return (sort==="name"?fileName(suggestedKeeper(a)).localeCompare(fileName(suggestedKeeper(b)),"pt-BR"):sort==="copies"?b.files.length-a.files.length:volume(b)-volume(a))||a.sha256.localeCompare(b.sha256);
  });
}
export function exportReview(groups:DuplicateGroup[],choices:Record<string,string>){
  return JSON.stringify({version:1,kind:"acervo-duplicate-review",readOnly:true,groups:groups.map(group=>{
    const plan=reviewGroup(group,choices[group.sha256]);
    return {sha256:group.sha256,bytes:group.bytes,keep:plan.keeper,review:plan.reviewable.map(file=>file.path),protected:plan.blocked.map(file=>({path:file.path,reasons:protections(file)}))};
  })},null,2);
}
