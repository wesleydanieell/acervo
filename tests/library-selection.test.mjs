import test from "node:test";
import assert from "node:assert/strict";
import { selectVisibleRange } from "../src/library-selection.ts";

const visible = ["Filosofia/a.epub", "Filosofia/b.pdf", "Literatura/c.epub", "Literatura/d.pdf"];

test("shift selects a contiguous interval in both directions", () => {
  assert.deepEqual([...selectVisibleRange(visible, visible[1], visible[3], new Set([visible[0]]), false)],
    [visible[1], visible[2], visible[3]]);
  assert.deepEqual([...selectVisibleRange(visible, visible[3], visible[1], new Set(), false)],
    [visible[1], visible[2], visible[3]]);
});

test("ctrl+shift extends a previous selection", () => {
  const selected = new Set([visible[0]]);
  const result = selectVisibleRange(visible, visible[2], visible[3], selected, true);
  assert.deepEqual([...result], [visible[0], visible[2], visible[3]]);
  assert.equal(selected.size, 1);
});

test("a stale anchor chooses only the visible target", () => {
  assert.deepEqual([...selectVisibleRange(visible, "removed.epub", visible[2], new Set([visible[0]]), false)],
    [visible[2]]);
  assert.deepEqual([...selectVisibleRange(visible, null, visible[2], new Set([visible[0]]), true)],
    [visible[0], visible[2]]);
});

import {updateSelection,selectionSummary,nextSelectionBatch,folderContains} from "../src/library-selection.ts";
test("selection operations preserve items outside the current filter",()=>{
 const selected=new Set(["hidden","a"]);
 const inverted=updateSelection(selected,["a","b","b"],"invert");
 assert.deepEqual([...inverted],["hidden","b"]);
 assert.deepEqual(selectionSummary(inverted,["a","b"]),{total:2,inResults:1,outside:1});
 assert.deepEqual([...updateSelection(inverted,["b"],"remove")],["hidden"]);
});
test("batch uses deterministic library order with explicit limit",()=>{
 const paths=Array.from({length:220},(_,i)=>"Book "+i);
 const selected=new Set([...paths].reverse());
 const batch=nextSelectionBatch(selected,paths);
 assert.deepEqual(batch,paths.slice(0,100));
 assert.equal(updateSelection(selected,batch,"remove").size,120);
});
test("folder scope checks path boundaries and root direct mode",()=>{
 assert.equal(folderContains("A","A/B",true),true);
 assert.equal(folderContains("A","AB/B",true),false);
 assert.equal(folderContains("A","A/B",false),false);
 assert.equal(folderContains("","A",true),true);
 assert.equal(folderContains("","A",false),false);
});
