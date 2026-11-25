import {test} from "node:test";
import assert from "node:assert/strict";
import {resolvePalette,interpolateColor,interpolatePalette,paletteEqual,paletteKeys} from "../src/palette-motion.ts";
const theme={mode:"dark",primary:"#aaffcc",on_primary:"#112233",surface:"#121212",on_surface:"#eeeeee",surface_container:"#222222",outline_variant:"#555555"};
test("Noctalia resolves every shared token and manual palettes ignore automatic updates",()=>{
 const a=resolvePalette("noctalia",theme);
 assert.equal(a.colors["--accent"],theme.primary);
 assert.equal(a.colors["--glass-strong"],theme.surface_container);
 assert.equal(Object.keys(a.colors).length,paletteKeys.length);
 assert.ok(paletteEqual(resolvePalette("rose",theme),resolvePalette("rose",null)));
 assert.equal(resolvePalette("noctalia",{...theme,primary:"url(invalid)"}).colors["--accent"],"#8ddbb8");
});
test("interpolates color and alpha with exact endpoints",()=>{
 assert.equal(interpolateColor("#000000","#ffffff",.5),"#808080ff");
 assert.equal(interpolateColor("#ffffff00","#ffffff",.5),"#ffffff80");
 assert.equal(interpolateColor("#ffffff00","#ffffff",1),"#ffffff");
 const a=resolvePalette("noctalia",theme),b=resolvePalette("noctalia",{...theme,primary:"#112233"});
 assert.notEqual(interpolatePalette(a,b,.5).colors["--accent"],b.colors["--accent"]);
 assert.ok(paletteEqual(interpolatePalette(a,b,1),b));
});
test("light/dark changes stay atomic rather than mixing foreground with incompatible surfaces",()=>{
 const a=resolvePalette("noctalia",theme),b=resolvePalette("noctalia",{...theme,mode:"light"});
 assert.ok(paletteEqual(interpolatePalette(a,b,.01),b));
});
