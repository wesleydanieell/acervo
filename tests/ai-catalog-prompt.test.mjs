import { strict as assert } from "node:assert";
import { test } from "node:test";
import { CATALOG_PROMPT_BATCH, buildCatalogPrompt } from "../src/ai-catalog-prompt.ts";

test("Prompt keeps paths and separates metadata cataloging from moving files", () => {
  const input = [{ path: "Ciências/Física/Termodinâmica.epub", title: "Termodinâmica", format: "EPUB", folder: "Ciências/Física" }];
  const prompt = buildCatalogPrompt(input, ["Ciências", "Ciências/Física"]);
  assert.match(prompt, /Preserve CADA caminho original exatamente/);
  assert.match(prompt, /NÃO movimenta arquivos/);
  assert.match(prompt, /"caminho": "Ciências\/Física\/Termodinâmica.epub"/);
  assert.match(prompt, /"pasta_atual": "Ciências\/Física"/);
  assert.deepEqual(input[0].path, "Ciências/Física/Termodinâmica.epub");
});

test("Limits exported files to one manageable batch of 50", () => {
  const items = Array.from({ length: CATALOG_PROMPT_BATCH + 1 }, (_, i) => ({
    path: `Livros/Obra-${i}.pdf`, title: `Obra-${i}`, folder: "Livros", format: "PDF",
  }));
  const prompt = buildCatalogPrompt(items, ["Livros"]);
  const content = JSON.parse(prompt.split("INVENTÁRIO DA BIBLIOTECA:\n\n")[1]);
  assert.equal(content.arquivos_sem_ficha.length, CATALOG_PROMPT_BATCH);
  assert.equal(content.arquivos_sem_ficha[0].caminho, "Livros/Obra-0.pdf");
  assert.equal(content.arquivos_sem_ficha.at(-1).caminho, "Livros/Obra-49.pdf");
});
