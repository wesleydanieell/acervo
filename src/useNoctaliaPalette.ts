import {useEffect,useState} from "react";
import {invoke} from "@tauri-apps/api/core";
import {listen} from "@tauri-apps/api/event";
import type {ThemeTokens} from "./palette-motion";

/** Native invalidations are hints; polling covers missing watcher directories,
 * transient writes, and events that arrive while the app is suspended. */
export function useNoctaliaPalette(){
  const [theme,setTheme]=useState<ThemeTokens|null>(null);
  useEffect(()=>{
    let disposed=false,running=false,queued=false,misses=0;
    let timer:number|undefined;
    const read=async(poll:boolean)=>{
      if(disposed)return;
      if(running){queued=true;return;}
      running=true;
      try{
        const tokens=await invoke<ThemeTokens|null>("read_noctalia_colors");
        if(disposed)return;
        if(tokens){
          misses=0;
          setTheme(previous=>JSON.stringify(previous)===JSON.stringify(tokens)?previous:tokens);
        }else if(poll&&++misses>=3)setTheme(null);
      }catch{
        if(!disposed&&poll&&++misses>=3)setTheme(null);
      }finally{
        running=false;
        if(queued&&!disposed){queued=false;schedule();}
      }
    };
    const schedule=()=>{
      if(disposed)return;
      if(timer!==undefined)window.clearTimeout(timer);
      timer=window.setTimeout(()=>{timer=undefined;void read(false);},100);
    };
    const subscription=listen("acervo-noctalia-changed",schedule);
    void subscription.then(()=>schedule()).catch(()=>{});
    void read(true);
    const fallback=window.setInterval(()=>void read(true),2000);
    const onFocus=()=>schedule();
    window.addEventListener("focus",onFocus);
    return()=>{
      disposed=true;
      if(timer!==undefined)window.clearTimeout(timer);
      window.clearInterval(fallback);
      window.removeEventListener("focus",onFocus);
      void subscription.then(unlisten=>unlisten()).catch(()=>{});
    };
  },[]);
  return theme;
}
