import test from "node:test";
import assert from "node:assert/strict";
import { catalogToCsv, csvToCatalogJson } from "../src/catalog-csv.ts";

test("CSV round-trip keeps books, quoted fields, paths and guards spreadsheet formulas", () => {
  const works = [{
    title: '=SUM(1,2) O "Banquete"',
    author: "Platão\nTradução",
    publishers: '["Editora, A","Editora B"]',
    destination: "Literatura/Filosofia",
    priority: true,
    associatedFiles: ["Literatura/Filosofia/O Banquete.epub", "PDF, edição.pdf"],
  }];
  const csv = catalogToCsv(works);
  assert.ok(csv.includes("\"'=SUM"));
  const data = JSON.parse(csvToCatalogJson(csv));
  assert.equal(data.versao, 1);
  assert.equal(data.obras.length, 1);
  assert.equal(data.obras[0].titulo, works[0].title);
  assert.equal(data.obras[0].autor, works[0].author);
  assert.deepEqual(data.obras[0].editoras_indicadas, ["Editora, A", "Editora B"]);
  assert.deepEqual(data.obras[0].arquivos_encontrados, works[0].associatedFiles);
  assert.equal(data.obras[0].prioridade_alta, 1);
});

test("semicolon CSV from spreadsheet converts into validated JSON", () => {
  const csv = '\uFEFFtítulo;autor;editoras_indicadas;local_indicado;prioridade_alta\n"Dom Casmurro";"Machado de Assis";"Editora A | Editora B";"Literatura/Brasileira";1\r\n';
  const data = JSON.parse(csvToCatalogJson(csv));
  assert.equal(data.obras[0].titulo, "Dom Casmurro");
  assert.deepEqual(data.obras[0].editoras_indicadas, ["Editora A", "Editora B"]);
  assert.equal(data.obras[0].local_indicado, "Literatura/Brasileira");
});

test("rejects path escapes, corrupt quoting and ambiguous columns before import", () => {
  assert.throws(() => csvToCatalogJson("titulo,local_indicado\nLivro,../../fora\n"), /pasta inválida/);
  assert.throws(() => csvToCatalogJson("titulo,arquivos_encontrados\nLivro,../sombra.pdf\n"), /caminho de arquivo inválido/);
  assert.throws(() => csvToCatalogJson('titulo\n"Livro sem fim'), /aspas não fechadas/);
  assert.throws(() => csvToCatalogJson("titulo,titulo\nLivro,Outro"), /Colunas inválidas/);
  assert.throws(() => csvToCatalogJson("titulo,autor\nLivro,Autor,Extra"), /colunas extras/);
  assert.throws(() => csvToCatalogJson("autor\nPessoa"), /coluna titulo/);
});
