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
