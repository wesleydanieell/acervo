import { strict as assert } from "node:assert";
import { test } from "node:test";
import { mergeSourcePage, resultIdentity } from "../src/discovery-pagination.ts";

const result = (overrides = {}) => ({
  sourceId: "a", sourceName: "Source A", title: "Book", author: "Author",
  language: "pt", format: "epub", downloadUrl: "https://example.com/book",
  ...overrides,
});

test("merges only the requested source and preserves other sources and cursors", () => {
  const existing = result({ sourceId: "b" });
  const current = { results: [existing], errors: [], nextPages: { a: "a1", b: "b1" } };
  const incoming = {
    results: [result(), result({ sourceId: "b", title: "Foreign" })], errors: [],
    nextPages: { a: "a2", b: "foreign", c: "foreign" },
  };
  const merged = mergeSourcePage(current, incoming, "a");
  assert.deepEqual(merged.results, [existing, result()]);
  assert.deepEqual(merged.nextPages, { a: "a2", b: "b1" });
  assert.deepEqual(current.nextPages, { a: "a1", b: "b1" });
});

test("stable identities deduplicate remote IDs and fallback fields exactly", () => {
  const original = result({ remoteId: "123" });
  const variants = [
    result({ remoteId: "123", format: "pdf" }),
    result({ remoteId: "123", downloadUrl: "https://example.com/other" }),
    result({ remoteId: "456" }),
    result(), result({ author: "Other" }), result({ language: "en" }),
    result({ title: "Other" }),
  ];
  const merged = mergeSourcePage({ results: [original], errors: [] }, {
    results: [result({ ...original, title: "Updated metadata" }), ...variants,
      result({ remoteId: "" }), ...variants], errors: [],
  }, "a");
  assert.deepEqual(merged.results, [original, ...variants]);
  assert.equal(resultIdentity(result()), resultIdentity(result({ remoteId: "" })));
  assert.notEqual(resultIdentity(original), resultIdentity({ ...original, sourceId: "b" }));
  assert.notEqual(resultIdentity(result()), resultIdentity(result({ title: "book" })));
});

test("exhaustion, empty cursors, and repeated cursors clear only the requested cursor", () => {
  for (const nextPages of [undefined, {}, { a: "" }, { a: "a1" }, { b: "foreign" }]) {
    const current = { results: [], errors: [], nextPages: { a: "a1", b: "b1" } };
    assert.deepEqual(mergeSourcePage(current, { results: [], errors: [], nextPages }, "a").nextPages,
      { b: "b1" });
  }
});

test("page errors preserve results and the requested cursor for a successful retry", () => {
  const current = { results: [result()], errors: ["Earlier error"], nextPages: { a: "a1", b: "b1" } };
  const failed = mergeSourcePage(current, {
    results: [result({ title: "Partial" })], errors: ["Page failed"], nextPages: { a: "a2", b: "bad" },
  }, "a");
  assert.deepEqual(failed.results, current.results);
  assert.deepEqual(failed.nextPages, current.nextPages);
  assert.deepEqual(failed.errors, ["Earlier error", "Page failed"]);
  const retried = mergeSourcePage(failed, {
    results: [result({ title: "Next" })], errors: [], nextPages: { a: "a2" },
  }, "a");
  assert.equal(retried.results.length, 2);
  assert.deepEqual(retried.nextPages, { a: "a2", b: "b1" });
});
