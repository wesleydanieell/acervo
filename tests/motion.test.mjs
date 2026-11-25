import {test} from "node:test";
import assert from "node:assert/strict";
import {morphTransform,motion} from "../src/motion.ts";
test("morph calculates displacement and scale in the target coordinate system",()=>{
 assert.equal(morphTransform({left:10,top:20,width:100,height:50},{left:30,top:40,width:200,height:100}),"translate(-20px,-20px) scale(0.5,0.5)");
 assert.equal(morphTransform({left:10,top:20,width:100,height:50},{left:10.1,top:20,width:100,height:50}),null);
});
test("invalid geometry does not produce an animation",()=>{
 const a={left:0,top:0,width:100,height:50};
 assert.equal(morphTransform(a,{...a,width:0}),null);
 assert.equal(morphTransform({...a,top:NaN},a),null);
 assert.ok(motion.fast<motion.standard&&motion.standard<motion.emphasized);
});
