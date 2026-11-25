export type NamingPreset="title"|"authorTitle"|"titleAuthor";
export const namingOptions: {id:NamingPreset;label:string;example:string}[]=[
  {id:"title",label:"Título",example:"Dom Casmurro.epub"},
  {id:"authorTitle",label:"Autor - Título",example:"Machado de Assis - Dom Casmurro.epub"},
  {id:"titleAuthor",label:"Título - Autor",example:"Dom Casmurro - Machado de Assis.epub"},
];
export function readNamingPreset(value:string|null):NamingPreset {
  return value==="authorTitle"||value==="titleAuthor"?value:"title";
}
