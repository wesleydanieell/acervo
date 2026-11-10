import { strict as assert } from "node:assert";
import { test } from "node:test";
import { alternateEditions, indexEditions, titleKey } from "../src/edition-matches.ts";

test("Matches accents and punctuation but not unrelated works", () => {
  const files = [
    { path: "A/Dom-Casmurro.pdf", title: "Dom-Casmurro", format: "PDF" },
    { path: "A/Outra.epub", title: "Dom Quixote", format: "EPUB" },
  ];
  const matches = alternateEditions(
    { title: "Dóm Casmurro", associatedFiles: [] },
    indexEditions(files),
  );
  assert.deepEqual(matches.map((file) => file.path), ["A/Dom-Casmurro.pdf"]);
  assert.equal(titleKey(" Dóm   Casmurro! "), "dom casmurro");
});

test("A linked PDF only suggests an unlinked EPUB, never the same format", () => {
  const files = [
    { path: "A/Livro.pdf", title: "Livro", format: "PDF" },
    { path: "B/Livro.pdf", title: "Livro", format: "PDF" },
    { path: "C/Livro.epub", title: "Livro", format: "EPUB" },
  ];
  assert.deepEqual(
    alternateEditions({ title: "Livro", associatedFiles: ["A/Livro.pdf"] }, indexEditions(files))
      .map((file) => file.path),
    ["C/Livro.epub"],
  );
});

test("Do not claim a match for very short or merely similar titles", () => {
  const files = [
    { path: "A/It.pdf", title: "It", format: "PDF" },
    { path: "A/Outro.pdf", title: "Dom Casmurro Comentado", format: "PDF" },
  ];
  const index = indexEditions(files);
  assert.deepEqual(alternateEditions({ title: "It", associatedFiles: [] }, index), []);
  assert.deepEqual(alternateEditions({ title: "Dom Casmurro", associatedFiles: [] }, index), []);
});
