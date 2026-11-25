import {test} from "node:test";
import assert from "node:assert/strict";
import {listWindow,navigationIndex,scrollTarget} from "../src/virtual-list.ts";
test("large lists render a bounded window, including a partial row",()=>{
 assert.deepEqual(listWindow(150000,10005,800,100,6),{start:94,end:115});
 assert.deepEqual(listWindow(100,9900,800,100,6),{start:93,end:100});
});
test("list may be below or above the viewport",()=>{
 assert.deepEqual(listWindow(100,-900,800,100),{start:0,end:0});
 assert.deepEqual(listWindow(100,10001,800,100),{start:0,end:0});
 assert.deepEqual(listWindow(100,-150,800,100,6),{start:0,end:13});
});
test("keyboard targets clamp and scroll just enough to expose a row",()=>{
 assert.equal(navigationIndex("End",0,150000,8),149999);
 assert.equal(navigationIndex("Home",150,200,8),0);
 assert.equal(navigationIndex("PageUp",4,200,8),0);
 assert.equal(navigationIndex("ArrowDown",199,200,8),199);
 assert.equal(navigationIndex("Enter",3,10,8),null);
 assert.equal(scrollTarget(10,100,0,800),300);
 assert.equal(scrollTarget(10,100,1250,800),1000);
 assert.equal(scrollTarget(10,100,400,800),400);
});
