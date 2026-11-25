export type PaletteName="noctalia"|"jade"|"indigo"|"rose"|"amber";
export type ThemeTokens=Record<"primary"|"on_primary"|"surface"|"on_surface"|"surface_container"|"outline_variant",string>&{mode?:"light"|"dark"};
export const paletteKeys=["--accent","--accent-ink","--glass-base","--glass-strong","--text","--outline"] as const;
export type PaletteKey=typeof paletteKeys[number];
export type ResolvedPalette={mode:"light"|"dark";colors:Record<PaletteKey,string>};
const presets={jade:"#8ddbb8",indigo:"#a6b7ff",rose:"#f8a9c7",amber:"#f2c27c"};
const isHex=(value:unknown):value is string=>typeof value==="string"&&/^#[0-9a-f]{6}([0-9a-f]{2})?$/i.test(value);
export function resolvePalette(name:PaletteName,theme:ThemeTokens|null):ResolvedPalette{
  if(name==="noctalia"&&theme&&[theme.primary,theme.on_primary,theme.surface,theme.surface_container,theme.on_surface,theme.outline_variant].every(isHex)){
    return {mode:theme.mode==="light"?"light":"dark",colors:{"--accent":theme.primary,"--accent-ink":theme.on_primary,"--glass-base":theme.surface,"--glass-strong":theme.surface_container,"--text":theme.on_surface,"--outline":theme.outline_variant}};
  }
  return {mode:"dark",colors:{"--accent":presets[name==="noctalia"?"jade":name]??presets.jade,"--accent-ink":"#142b2c","--glass-base":"#101923","--glass-strong":"#1c2936","--text":"#ecf4f3","--outline":"#ffffff20"}};
}
function channels(hex:string){
  const full=hex.slice(1).padEnd(8,"f");return [0,2,4,6].map(index=>parseInt(full.slice(index,index+2),16));
}
export function interpolateColor(from:string,to:string,fraction:number){
  if(!isHex(from)||!isHex(to)||fraction>=1)return to;
  if(fraction<=0)return from;
  const a=channels(from),b=channels(to);
  return "#"+a.map((value,i)=>Math.round(value+(b[i]-value)*fraction).toString(16).padStart(2,"0")).join("");
}
export function interpolatePalette(from:ResolvedPalette,to:ResolvedPalette,fraction:number):ResolvedPalette{
  // A dark/light interpolation can pass through low-contrast grays.
  // Change the whole pair atomically instead.
  if(from.mode!==to.mode||fraction>=1)return to;
  return {mode:to.mode,colors:Object.fromEntries(paletteKeys.map(key=>[key,interpolateColor(from.colors[key],to.colors[key],fraction)])) as ResolvedPalette["colors"]};
}
export const paletteEqual=(a:ResolvedPalette,b:ResolvedPalette)=>a.mode===b.mode&&paletteKeys.every(key=>a.colors[key]===b.colors[key]);
