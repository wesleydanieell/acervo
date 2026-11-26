import test from "node:test";
import assert from "node:assert/strict";
import {draftOperation,draftParents,folderParent,projectFolders,structuralPath,impactCounts} from "../src/structural-plan.ts";
test("projection remaps descendants without touching similarly named folders",()=>{
  const folders=["A","A/Sub","AB","AB/Other","99 - Catálogo","99 - Catálogo/Private"];
  const ops=[{kind:"createFolder",path:"New"},{kind:"moveFolder",source:"A",target:"New/A"}];
  assert.deepEqual(projectFolders(folders,ops),["AB","AB/Other","New","New/A","New/A/Sub"]);
  assert.equal(folders.length,6);
});
test("draft operations reject overlapping and protected paths",()=>{
  assert.deepEqual(draftOperation("moveFolder","A","AB","Renamed"),{kind:"moveFolder",source:"A",target:"AB/Renamed"});
  assert.throws(()=>draftOperation("moveFolder","A","A/Sub","Nested"));
  assert.throws(()=>draftOperation("createFolder","","","../escape"));
  assert.equal(structuralPath("99 - Catálogo/Private"),false);
  assert.equal(structuralPath("A//B"),false);
  assert.equal(structuralPath("A/../B"),false);
  assert.equal(structuralPath("A/B"),true);
});
test("destination candidates exclude moved branches and permit proposed parents",()=>{
  assert.deepEqual(draftParents(["A","A/Sub","AB"],[{kind:"createFolder",path:"New"},{kind:"moveFolder",source:"A",target:"New/A"}],""),["AB","New"]);
});
test("impact totals include all diagnostic sources",()=>{
  const p={movedFolders:[{books:3},{books:2}],referenceImpact:{works:[{}],blockers:[{}]},downloadImpact:{blockers:[{}],destinationConflicts:[{}]}};
  assert.deepEqual(impactCounts(p),{books:5,works:1,blockers:3});
  assert.deepEqual(impactCounts({...p,downloadImpact:null}),{books:5,works:1,blockers:1});
});

test("rename defaults to the original parent and protected nested paths are excluded",()=>{
  assert.equal(folderParent("A/Child"),"A");
  assert.equal(folderParent("A"),"");
  assert.equal(structuralPath("A/99 – Catálogo"),false);
  assert.equal(structuralPath(Array(81).fill("a").join("/")),false);
  assert.throws(()=>draftOperation("createFolder","",Array(79).fill("longname").join("/"),"new"));
});
