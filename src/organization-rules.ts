export type CustomRule={name:string;enabled:boolean;format:string;author:string;language:string;publisher:string;destination:string};
export type OrganizationRules={catalog:boolean;format:string|null;destination:string|null;custom:CustomRule[]};
export const defaultRules=():OrganizationRules=>({catalog:true,format:null,destination:null,custom:[]});
export const ruleStorageKey=(root:string)=>"acervo-organization-rules-v1:"+root;
export function parseRules(raw:string|null):OrganizationRules{
  if(raw===null)return defaultRules();
  if(new TextEncoder().encode(raw).length>65536)throw new Error("Configurações de organização excedem o limite.");
  const value=JSON.parse(raw);
  if(!value||value.version!==1||typeof value.rules?.catalog!=="boolean"||!Array.isArray(value.rules.custom)||value.rules.custom.length>20)throw new Error("Configurações de organização inválidas.");
  const {catalog,format,destination,custom}=value.rules;
  if(!((format===null&&destination===null)||(typeof format==="string"&&typeof destination==="string")))throw new Error("Regra de formato inválida.");
  const fields=["name","format","author","language","publisher","destination"] as const;
  const checked=custom.map((rule:unknown)=>{
    if(!rule||typeof rule!=="object"||Array.isArray(rule))throw new Error("Regra inválida.");
    const item=rule as Record<string,unknown>;
    if(typeof item.enabled!=="boolean"||fields.some(key=>typeof item[key]!=="string")||Object.keys(item).length!==7)throw new Error("Regra inválida.");
    return {name:item.name,enabled:item.enabled,format:item.format,author:item.author,language:item.language,publisher:item.publisher,destination:item.destination} as CustomRule;
  });
  return {catalog,format,destination,custom:checked};
}
export function encodeRules(rules:OrganizationRules):string{
  const raw=JSON.stringify({version:1,rules});
  parseRules(raw);
  return raw;
}
export function newRule(existing:CustomRule[]):CustomRule{
  let index=1;
  const names=new Set(existing.map(rule=>rule.name.trim().toLowerCase()));
  while(names.has("regra "+index))index++;
  return {name:"Regra "+index,enabled:true,format:"",author:"",language:"",publisher:"",destination:""};
}
