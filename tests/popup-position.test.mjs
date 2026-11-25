import {test} from "node:test";
import assert from "node:assert/strict";
import {popupPosition} from "../src/popup-position.ts";
test("menu opens below its anchor and clamps horizontally",()=>{
 const p=popupPosition({left:280,right:320,top:50,bottom:88},200,160,320,500);
 assert.equal(p.left,112);assert.equal(p.top,94);assert.equal(p.above,false);
});
test("menu flips above a bottom anchor and caps scrolling height",()=>{
 const p=popupPosition({left:10,right:100,top:200,bottom:238},200,320,240,270);
 assert.equal(p.above,true);assert.equal(p.maxHeight,186);assert.equal(p.top,8);
});
