type Filters={language:string;format:string};
type Defaults={preferredLanguage:string;preferredFormat:string};

// Explicit catalog filters are authoritative. Only remove a default that was
// invalidated by an extension update and is still in use.
export function reconcileSourceFilters(current:Filters,previous:Defaults,next:Defaults,catalogSearch:boolean):Filters {
  if(catalogSearch)return current;
  return {
    language:previous.preferredLanguage && !next.preferredLanguage && current.language===previous.preferredLanguage ? "" : current.language,
    format:previous.preferredFormat && !next.preferredFormat && current.format===previous.preferredFormat ? "" : current.format,
  };
}
