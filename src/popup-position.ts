type Anchor={left:number;right:number;top:number;bottom:number};
export function popupPosition(anchor:Anchor,wantedWidth:number,wantedHeight:number,viewportWidth:number,viewportHeight:number){
  const gap=6,edge=8;
  const width=Math.min(wantedWidth,Math.max(0,viewportWidth-edge*2));
  const below=Math.max(0,viewportHeight-anchor.bottom-gap-edge);
  const aboveSpace=Math.max(0,anchor.top-gap-edge);
  const above=below<wantedHeight&&aboveSpace>below;
  const maxHeight=Math.min(wantedHeight,above?aboveSpace:below);
  return {left:Math.max(edge,Math.min(anchor.left,viewportWidth-edge-width)),top:above?Math.max(edge,anchor.top-gap-maxHeight):anchor.bottom+gap,width,maxHeight,above};
}
