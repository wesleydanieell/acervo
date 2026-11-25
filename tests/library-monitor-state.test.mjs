import {test} from "node:test";
import assert from "node:assert/strict";
import {nextMonitorGeneration} from "../src/library-monitor-state.ts";
test("monitor generations survive remount and clock rollback",()=>{
  let value=null;
  const storage={getItem:()=>value,setItem:(_key,next)=>{value=next;}};
  assert.equal(nextMonitorGeneration(storage,1000),1000);
  assert.equal(nextMonitorGeneration(storage,1000),1001);
  assert.equal(nextMonitorGeneration(storage,900),1002);
  value="invalid";assert.equal(nextMonitorGeneration(storage,1100),1100);
});
