import {test} from "node:test";
import assert from "node:assert/strict";
import {defaultRules,encodeRules,newRule,parseRules,ruleStorageKey} from "../src/organization-rules.ts";
test("per-library rules round trip without mixing libraries",()=>{
  const rules=defaultRules();
  rules.custom.push({...newRule([]),format:"EPUB",destination:"Literatura"});
  assert.deepEqual(parseRules(encodeRules(rules)),rules);
  assert.notEqual(ruleStorageKey("/library-a"),ruleStorageKey("/library-b"));
  assert.deepEqual(parseRules(null),defaultRules());
});
test("invalid and oversized persisted rules are rejected",()=>{
  for(const value of ["{","{}",JSON.stringify({version:2,rules:defaultRules()}),JSON.stringify({version:1,rules:{...defaultRules(),custom:[{}]}})," ".repeat(65537)])assert.throws(()=>parseRules(value));
});
test("new rule names do not duplicate existing names",()=>{
  assert.equal(newRule([{...newRule([]),name:"REGRA 1"}]).name,"Regra 2");
});
