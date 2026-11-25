import {test} from "node:test";
import assert from "node:assert/strict";
import {suggestedKeeper,reviewGroup,filterGroups,exportReview} from "../src/duplicate-review.ts";
const f=(path,extra={})=>({path,format:"PDF",associations:0,manualMetadata:false,editionMetadata:false,protectionUnknown:false,...extra});
const g={sha256:"abc",bytes:100,files:[f("B/Copia.pdf"),f("A/Livro.pdf",{associations:1}),f("C/Outra.pdf",{manualMetadata:true})]};
test("recommend a linked copy and protect other durable records",()=>{
 assert.equal(suggestedKeeper(g),"A/Livro.pdf");
 const plan=reviewGroup(g);
 assert.deepEqual(plan.reviewable.map(file=>file.path),["B/Copia.pdf"]);
 assert.deepEqual(plan.blocked.map(file=>file.path),["C/Outra.pdf"]);
 assert.equal(plan.logicalBytes,100);
 const changed=reviewGroup(g,"B/Copia.pdf");assert.equal(changed.reviewable.length,0);assert.equal(changed.blocked.length,2);
});
test("unknown protection never becomes a reviewable copy",()=>{
 const group={...g,files:[f("a.pdf",{protectionUnknown:true}),f("b.pdf",{protectionUnknown:true})]};
 assert.equal(reviewGroup(group).reviewable.length,0);
});
test("filters preserve entire matching groups and export non-destructive plan",()=>{
 assert.equal(filterGroups([g],"cópia","all","volume",{}).length,1);
 assert.equal(filterGroups([g],"missing","all","name",{}).length,0);
 const output=JSON.parse(exportReview([g],{}));assert.equal(output.readOnly,true);assert.deepEqual(output.groups[0].review,["B/Copia.pdf"]);
});
