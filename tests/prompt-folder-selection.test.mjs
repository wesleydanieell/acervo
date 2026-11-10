import test from "node:test";
import assert from "node:assert/strict";
import {
  chosenPromptFolders, directPromptChildren, promptFolderSelection, promptSubtree, togglePromptFolder,
} from "../src/prompt-folder-selection.ts";

const folders = [
  "01 - Literatura",
  "01 - Literatura/Brasileira",
  "01 - Literatura/Brasileira/Clássicos",
  "02 - Didáticos",
  "02 - Didáticos/Matemática",
  "03 - Entrada",
  "03 - Entrada/Para revisar",
  "010 - Não relacionado",
];

test("complete library includes all top-level folders 01 through 03 and descendants", () => {
  assert.deepEqual(chosenPromptFolders(folders, null), folders);
  assert.deepEqual(directPromptChildren(folders, ""), [
    "01 - Literatura", "02 - Didáticos", "03 - Entrada", "010 - Não relacionado",
  ]);
  assert.deepEqual(directPromptChildren(folders, "01 - Literatura"), ["01 - Literatura/Brasileira"]);
  assert.deepEqual(directPromptChildren(folders, "01 - Literatura/Brasileira"),
    ["01 - Literatura/Brasileira/Clássicos"]);
  assert.deepEqual(directPromptChildren(folders, "03 - Entrada/Para revisar"), []);
});

test("selecting a parent recursively includes every subfolder", () => {
  const selected = togglePromptFolder(folders, new Set(), "01 - Literatura");
  assert.deepEqual(chosenPromptFolders(folders, selected), folders.slice(0, 3));
  assert.equal(promptFolderSelection(folders, selected, "01 - Literatura"), "all");
  const removed = togglePromptFolder(folders, selected, "01 - Literatura");
  assert.deepEqual(chosenPromptFolders(folders, removed), []);
});

test("deselecting a parent from all removes its subtree, not a similarly prefixed folder", () => {
  const selected = togglePromptFolder(folders, null, "01 - Literatura");
  assert.deepEqual(chosenPromptFolders(folders, selected), folders.slice(3));
  assert.equal(promptFolderSelection(folders, selected, ""), "partial");
  assert.equal(promptFolderSelection(folders, selected, "01 - Literatura"), "none");
  assert.equal(promptFolderSelection(folders, selected, "010 - Não relacionado"), "all");
  assert.equal(promptSubtree(folders, "01 - Literatura").length, 3);
});

test("partial selection is indicated and a parent click fills missing descendants", () => {
  const selected = new Set(["01 - Literatura/Brasileira"]);
  assert.equal(promptFolderSelection(folders, selected, "01 - Literatura"), "partial");
  const complete = togglePromptFolder(folders, selected, "01 - Literatura");
  assert.deepEqual(chosenPromptFolders(folders, complete), folders.slice(0, 3));
  assert.equal(promptFolderSelection(folders, complete, "01 - Literatura"), "all");
});

test("root selection toggles the entire library and unknown paths are ignored", () => {
  assert.deepEqual(chosenPromptFolders(folders, togglePromptFolder(folders, null, "")), []);
  assert.equal(togglePromptFolder(folders, new Set(), ""), null);
  assert.deepEqual(chosenPromptFolders(folders, togglePromptFolder(folders, new Set(), "fake")), []);
});
