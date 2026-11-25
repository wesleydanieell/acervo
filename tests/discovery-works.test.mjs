import { strict as assert } from "node:assert";
import { test } from "node:test";
import { registerHooks } from "node:module";

// Production imports use the bundler's extensionless convention.
registerHooks({
  resolve(specifier, context, nextResolve) {
    return nextResolve(specifier === "./discovery-pagination"
      ? "./discovery-pagination.ts" : specifier, context);
  },
});
const { groupDiscoveryWorks } = await import("../src/discovery-works.ts");
const { resultIdentity } = await import("../src/discovery-pagination.ts");

const result = (overrides = {}) => ({
  sourceId: "a", sourceName: "Source A", title: "Book", author: "Author",
  remoteId: "123", language: "pt", format: "epub",
  downloadUrl: "https://example.com/book", ...overrides,
});

test("groups editions while preserving format, language, and input order", () => {
  const rows = ["pdf", "EPUB", "mobi", "AZW3", "djvu"].map(format => result({ format }));
  rows.push(result({ language: "en" }));
  const metadata = result({ format: "html" });
  const [work] = groupDiscoveryWorks([...rows, metadata]);
  assert.deepEqual(work.editions, rows);
  assert.deepEqual(work.metadata, [metadata]);
  assert.equal(work.title, "Book");
  assert.equal(work.author, "Author");
  assert.equal(work.sourceId, "a");
  assert.equal(work.sourceName, "Source A");
});

test("no-ID rows stay separate except matching result identities", () => {
  const first = result({ remoteId: undefined });
  const rows = [first, result({ remoteId: "", format: "pdf" }),
    result({ remoteId: undefined, language: "en" }),
    result({ remoteId: undefined, downloadUrl: "other" })];
  assert.equal(resultIdentity(first), resultIdentity({ ...first, remoteId: "" }));
  const works = groupDiscoveryWorks([...rows, { ...first, remoteId: "" }]);
  assert.equal(works.length, 4);
  assert.deepEqual(works.map(work => work.editions), rows.map(row => [row]));
});

test("same remote ID with conflicting title or author stays separate", () => {
  const rows = [result(), result({ title: "book" }), result({ author: "Other" })];
  const works = groupDiscoveryWorks(rows);
  assert.equal(works.length, 3);
  assert.equal(new Set(works.map(work => work.key)).size, 3);
  assert.deepEqual(works.map(work => work.editions), rows.map(row => [row]));
});

test("sources and different remote IDs never merge", () => {
  assert.equal(groupDiscoveryWorks([result(), result({ sourceId: "b" }),
    result({ remoteId: "456" })]).length, 3);
});

test("metadata-only results include missing links and unsupported formats", () => {
  const rows = [result({ downloadUrl: "" }), result({ format: "html" }),
    result({ format: "" }), result({ format: "zip" })];
  const [work] = groupDiscoveryWorks(rows);
  assert.deepEqual(work.editions, []);
  assert.deepEqual(work.metadata, rows);
});

test("drops exact duplicates without mutating input", () => {
  const row = Object.freeze(result());
  const rows = Object.freeze([row, Object.freeze({ ...row })]);
  assert.deepEqual(groupDiscoveryWorks(rows)[0].editions, [row]);
  assert.deepEqual(groupDiscoveryWorks([]), []);
});

test("keys are stable, collision-safe, and safe for prototype-like values", () => {
  const rows = [result({ sourceId: "__proto__", remoteId: "constructor" }),
    result({ sourceId: "a,b", remoteId: "c" }),
    result({ sourceId: "a", remoteId: "b,c" })];
  const works = groupDiscoveryWorks(rows);
  assert.equal(new Set(works.map(work => work.key)).size, 3);
  assert.deepEqual(groupDiscoveryWorks([...rows].reverse()).map(work => work.key),
    works.map(work => work.key).reverse());
});

test("different publishers and edition labels stay distinct during grouping", () => {
  const first = result({publisher:"Publisher A", editionLabel:"Revised"});
  const second = result({publisher:"Publisher B", editionLabel:"Original"});
  assert.notEqual(resultIdentity(first), resultIdentity(second));
  assert.deepEqual(groupDiscoveryWorks([first,second,first])[0].editions,[first,second]);
});
