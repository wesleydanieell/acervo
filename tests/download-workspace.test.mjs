import test from "node:test";
import assert from "node:assert/strict";
import { downloadGroup, downloadDestination } from "../src/download-workspace.ts";

test("download states map to exclusive groups", () => {
  for (const state of ["queued", "downloading", "pausing", "paused", "saving"]) assert.equal(downloadGroup(state), "active");
  assert.equal(downloadGroup("ready"), "ready");
  for (const state of ["saved", "failed", "cancelled"]) assert.equal(downloadGroup(state), "history");
});

test("a missing planned folder must not silently become the root", () => {
  assert.equal(downloadDestination("Missing", undefined, ["Books"]), null);
  assert.equal(downloadDestination("Missing", "", ["Books"]), "");
  assert.equal(downloadDestination("Books", undefined, ["Books"]), "Books");
  assert.equal(downloadDestination("", undefined, ["Books"]), "");
  assert.equal(downloadDestination("Books", "Removed", ["Books"]), null);
});

test("custom destinations require explicit creation mode and safe relative paths", async () => {
  const { validDownloadPath, planDownloadDestination } = await import("../src/download-workspace.ts");
  for (const path of ["", "/outside", "Books/../Escape", "Books//Other", "Books/.hidden", "99 — Catálogo/X", "Books/node_modules", "Books\\\\Windows", " Books"]) {
    assert.equal(validDownloadPath(path), false, path);
  }
  assert.equal(validDownloadPath("Literatura/Clássicos"), true);
  assert.equal(planDownloadDestination("Missing", undefined, undefined, ["Books"]), null);
  assert.deepEqual(planDownloadDestination("Missing", undefined, "Missing/History", ["Books"]), { path: "Missing/History", createMissing: true });
  assert.deepEqual(planDownloadDestination("Missing", "", undefined, ["Books"]), { path: "", createMissing: false });
});


test("catalog-linked downloads require the original library", async () => {
  const { canImportDownload } = await import("../src/download-workspace.ts");
  const target = { root: "/books", title: "Book", author: "", destination: "History" };
  assert.equal(canImportDownload("/books", target), true);
  assert.equal(canImportDownload("/other", target), false);
  assert.equal(canImportDownload("", null), false);
  assert.equal(canImportDownload("/other", null), true);
});
