// CSV interchange for visible catalog fields. Import always goes through the JSON validator.
export type CsvCatalogWork = {
  title: string;
  author: string;
  publishers: string;
  destination: string;
  priority: boolean;
  associatedFiles: string[];
};

const COLUMNS = ["titulo", "autor", "editoras_indicadas", "local_indicado", "prioridade_alta", "arquivos_encontrados"] as const;
const MAX_CSV_BYTES = 12_000_000;
const MAX_ROWS = 20_000;

function csvEscape(raw: string): string {
  // Prevent spreadsheet formula execution when users open exports in LibreOffice/Excel.
  const value = /^[\s]*[=+\-@]/.test(raw) ? "'" + raw : raw;
  return '"' + value.replace(/"/g, '""') + '"';
}

function undoFormulaEscape(raw: string): string {
  return raw.startsWith("'") && /^[\s]*[=+\-@]/.test(raw.slice(1)) ? raw.slice(1) : raw;
}

function publisherNames(raw: string): string[] {
  if (!raw.trim()) return [];
  try {
    const parsed: unknown = JSON.parse(raw);
    if (Array.isArray(parsed)) return parsed.filter((value): value is string => typeof value === "string");
  } catch { /* Legacy publishers can be plain text. */ }
  return [raw];
}

export function catalogToCsv(works: CsvCatalogWork[]): string {
  const rows = works.map((work) => [
    work.title, work.author, JSON.stringify(publisherNames(work.publishers)),
    work.destination, work.priority ? "1" : "0", JSON.stringify(work.associatedFiles),
  ].map(csvEscape).join(","));
  return [COLUMNS.join(","), ...rows].join("\r\n") + "\r\n";
}

function separatorOf(text: string): "," | ";" {
  let quoted = false;
  let commas = 0;
  let semicolons = 0;
  for (let index = 0; index < text.length; index++) {
    const char = text[index];
    if (char === '"') {
      if (quoted && text[index + 1] === '"') index++;
      else quoted = !quoted;
    } else if (!quoted && (char === "\n" || char === "\r")) break;
    else if (!quoted && char === ",") commas++;
    else if (!quoted && char === ";") semicolons++;
  }
  return semicolons > commas ? ";" : ",";
}

function rowsOf(text: string, separator: string): string[][] {
  const rows: string[][] = [];
  let row: string[] = [];
  let cell = "";
  let quoted = false;
  let endedQuote = false;
  for (let index = 0; index < text.length; index++) {
    const char = text[index];
    if (quoted) {
      if (char === '"' && text[index + 1] === '"') { cell += '"'; index++; }
      else if (char === '"') { quoted = false; endedQuote = true; }
      else cell += char;
    } else if (char === '"' && !cell) {
      quoted = true;
    } else if (char === separator || char === "\r" || char === "\n") {
      row.push(cell);
      cell = "";
      endedQuote = false;
      if (char !== separator) {
        if (row.some((value) => value.trim())) rows.push(row);
        if (rows.length > MAX_ROWS + 1) throw new Error("CSV excede 20 mil obras.");
        row = [];
        if (char === "\r" && text[index + 1] === "\n") index++;
      }
    } else if (endedQuote && char !== " " && char !== "\t") {
      throw new Error("CSV possui conteúdo após uma célula entre aspas.");
    } else if (!endedQuote) {
      if (char === '"') throw new Error("Aspas inesperadas no CSV.");
      cell += char;
    }
  }
  if (quoted) throw new Error("CSV com aspas não fechadas.");
  if (row.length || cell.trim()) {
    row.push(cell);
    if (row.some((value) => value.trim())) rows.push(row);
  }
  return rows;
}

function listField(raw: string, column: string): string[] {
  if (!raw.trim()) return [];
  if (raw.trim().startsWith("[")) {
    let data: unknown;
    try { data = JSON.parse(raw); }
    catch { throw new Error("Lista JSON inválida na coluna " + column + "."); }
    if (!Array.isArray(data) || data.some((value) => typeof value !== "string"))
      throw new Error("A coluna " + column + " deve conter uma lista de textos.");
    return data;
  }
  return raw.split("|").map((value) => value.trim()).filter(Boolean);
}

function safeFolder(value: string): boolean {
  return !value.startsWith("/") && !value.includes("\\") &&
    !/^[A-Za-z]:/.test(value) && !value.split("/").includes("..") &&
    !/[\x00-\x1f]/.test(value) && value.length <= 600;
}

export function csvToCatalogJson(csvText: string): string {
  const text = csvText.replace(/^\uFEFF/, "");
  if (new TextEncoder().encode(text).length > MAX_CSV_BYTES) throw new Error("CSV maior que 12 MB.");
  const rows = rowsOf(text, separatorOf(text));
  if (!rows.length) throw new Error("CSV vazio.");
  const headings = rows[0].map((value) => value.trim().normalize("NFD").replace(/[\u0300-\u036f]/g, "").toLowerCase());
  if (new Set(headings).size !== headings.length || headings.some((value) => !COLUMNS.includes(value as typeof COLUMNS[number])))
    throw new Error("Colunas inválidas ou repetidas. Use: " + COLUMNS.join(", "));
  if (!headings.includes("titulo")) throw new Error("CSV precisa ter a coluna titulo.");
  if (rows.length - 1 > MAX_ROWS) throw new Error("CSV excede 20 mil obras.");
  const find = (row: string[], name: typeof COLUMNS[number]) =>
    undoFormulaEscape(row[headings.indexOf(name)] || "").trim();
  const obras = rows.slice(1).map((row, index) => {
    if (row.length > headings.length) throw new Error("Linha " + (index + 2) + ": colunas extras.");
    const titulo = find(row, "titulo");
    const autor = find(row, "autor");
    const local_indicado = find(row, "local_indicado");
    const priority = find(row, "prioridade_alta").toLowerCase();
    if (!titulo || titulo.length > 500 || autor.length > 500)
      throw new Error("Linha " + (index + 2) + ": título ou autor inválido.");
    if (!safeFolder(local_indicado)) throw new Error("Linha " + (index + 2) + ": pasta inválida.");
    if (!["", "0", "1", "true", "false"].includes(priority))
      throw new Error("Linha " + (index + 2) + ": prioridade deve ser 0 ou 1.");
    const editoras_indicadas = listField(find(row, "editoras_indicadas"), "editoras_indicadas");
    const arquivos_encontrados = listField(find(row, "arquivos_encontrados"), "arquivos_encontrados");
    if (arquivos_encontrados.some((file) => !file || !safeFolder(file)))
      throw new Error("Linha " + (index + 2) + ": caminho de arquivo inválido.");
    return { titulo, autor, editoras_indicadas, local_indicado,
      prioridade_alta: priority === "1" || priority === "true" ? 1 : 0, arquivos_encontrados };
  });
  return JSON.stringify({ versao: 1, obras }, null, 2);
}
