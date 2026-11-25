import {test} from "node:test";
import assert from "node:assert/strict";
import {effectiveMetadata,inheritedMetadata,prepareManualMetadata} from "../src/manual-metadata.ts";
test("manual fields override acquired and embedded fields independently",()=>{
 const file={title:"Filename",edition:{metadata:{title:"Source title",author:"Source author",language:"",publisher:""}},embedded:{metadata:{title:"Embedded",author:"Embedded author",language:"pt",isbn:"123"}},manual:{metadata:{title:"Manual title",author:null,publisher:"Manual publisher"}}};
 assert.equal(inheritedMetadata(file).title,"Source title");
 assert.deepEqual(effectiveMetadata(file),{title:"Manual title",author:"Source author",language:"pt",publisher:"Manual publisher",isbn:"123",description:""});
});
test("empty fields inherit and byte bounds are enforced",()=>{
 const metadata=prepareManualMetadata({title:"  Book  ",author:"  "});
 assert.equal(prepareManualMetadata({description:"Line one\nLine two"}).description,"Line one Line two");
 assert.equal(metadata.title,"Book");assert.equal(metadata.author,null);
 assert.throws(()=>prepareManualMetadata({title:"é".repeat(601)}));
 assert.throws(()=>prepareManualMetadata({author:"a\u0000b"}));
});
