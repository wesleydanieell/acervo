export const metadataFields = [
  ["title","Título",1200],["author","Autor",1200],["language","Idioma",100],
  ["publisher","Editora",1200],["isbn","ISBN",100],["description","Descrição",16000],
] as const;
export type MetadataKey=typeof metadataFields[number][0];
export type ManualMetadata=Partial<Record<MetadataKey,string|null>>;
export type MetadataFile={title:string;manual?:{metadata:ManualMetadata};edition?:{metadata:{title:string;author:string;language:string;publisher:string}};embedded?:{metadata?:Partial<Record<MetadataKey,string>>|null}};
export function inheritedMetadata(file:MetadataFile):Record<MetadataKey,string>{
  const edition=file.edition?.metadata;
  const embedded=file.embedded?.metadata;
  return {
    title:edition?.title||embedded?.title||file.title,
    author:edition?.author||embedded?.author||"",
    language:edition?.language||embedded?.language||"",
    publisher:edition?.publisher||embedded?.publisher||"",
    isbn:embedded?.isbn||"",description:embedded?.description||"",
  };
}
export function effectiveMetadata(file:MetadataFile):Record<MetadataKey,string>{
  const data=inheritedMetadata(file);
  for(const [field] of metadataFields) {
    const value=file.manual?.metadata[field];
    if(value?.trim())data[field]=value;
  }
  return data;
}
export function prepareManualMetadata(values:ManualMetadata):ManualMetadata{
  const result:ManualMetadata={};
  for(const [field,label,limit] of metadataFields){
    const raw=values[field]??"";
    const value=(field==="description"?raw.replace(/\r\n|[\r\n\t]/g," "):raw).trim();
    if(new TextEncoder().encode(value).length>limit)throw new Error(label+": limite de "+limit+" bytes.");
    if(/[\u0000-\u001f\u007f-\u009f]/u.test(value))throw new Error(label+": caracteres de controle não são permitidos.");
    result[field]=value||null;
  }
  return result;
}
