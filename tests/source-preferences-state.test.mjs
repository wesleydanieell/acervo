import { test } from "node:test";
import assert from "node:assert/strict";
import {reconcileSourceFilters} from "../src/source-preferences-state.ts";

const previous={preferredLanguage:"pt",preferredFormat:"epub"};
const removed={preferredLanguage:"",preferredFormat:""};
test("repository updates clear only defaults still in use",()=>{
  assert.deepEqual(reconcileSourceFilters({language:"pt",format:"epub"},previous,removed,false),{language:"",format:""});
  assert.deepEqual(reconcileSourceFilters({language:"en",format:"epub"},previous,removed,false),{language:"en",format:""});
});
test("catalog searches preserve explicit filters during preference reconciliation",()=>{
  assert.deepEqual(reconcileSourceFilters({language:"pt",format:"epub"},previous,removed,true),{language:"pt",format:"epub"});
});
test("unchanged or changed defaults do not rewrite current searches",()=>{
  assert.deepEqual(reconcileSourceFilters({language:"pt",format:"epub"},previous,{preferredLanguage:"en",preferredFormat:"pdf"},false),{language:"pt",format:"epub"});
});
