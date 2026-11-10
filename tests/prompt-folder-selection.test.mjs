import test from "node:test";
import assert from "node:assert/strict";
import { chosenPromptFolders, togglePromptFolder } from "../src/prompt-folder-selection.ts";

const folders = ["Literatura", "Literatura/Brasileira", "Didáticos", "Didáticos/Matemática"];

test("all, none and explicit folders preserve the library ordering", () => {
  assert.deepEqual(chosenPromptFolders(folders, null), folders);
  assert.deepEqual(chosenPromptFolders(folders, new Set()), []);
  assert.deepEqual(chosenPromptFolders(folders, new Set(["Didáticos", "Literatura"])),
    ["Literatura", "Didáticos"]);
});

test("first toggle out of all selects everything else without mutating the source", () => {
  const selected = togglePromptFolder(folders, null, "Didáticos");
  assert.deepEqual(chosenPromptFolders(folders, selected),
    ["Literatura", "Literatura/Brasileira", "Didáticos/Matemática"]);
  assert.deepEqual(chosenPromptFolders(folders, null), folders);
  assert.equal(togglePromptFolder(folders, selected, "foreign").size, selected.size);
});
