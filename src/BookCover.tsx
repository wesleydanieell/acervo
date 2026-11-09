import { useEffect, useRef, useState } from "react";
import { Book } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import "./book-covers.css";

type LocalBook = { path: string; format: string };
type Props = { root: string; file?: LocalBook; title: string; format: string; present: boolean };

const MAX_CONCURRENT = 3;
const MAX_CACHE = 256;
const cache = new Map<string, string | null>();
const pending = new Map<string, Promise<string | null>>();
const jobs: (() => void)[] = [];
let running = 0;

function drain() {
  while (running < MAX_CONCURRENT && jobs.length) {
    const next = jobs.shift();
    if (!next) break;
    running++;
    next();
  }
}

function fetchCover(root: string, file: LocalBook): Promise<string | null> {
  const key = root + "\u0000" + file.path;
  if (cache.has(key)) return Promise.resolve(cache.get(key) ?? null);
  const underway = pending.get(key);
  if (underway) return underway;
  const request = new Promise<string | null>((resolve) => {
    jobs.push(() => {
      invoke<string | null>("get_book_cover", { root, relativePath: file.path })
        .catch(() => null)
        .then((value) => {
          cache.delete(key);
          cache.set(key, value);
          if (cache.size > MAX_CACHE) cache.delete(cache.keys().next().value!);
          resolve(value);
        })
        .finally(() => { pending.delete(key); running--; drain(); });
    });
    drain();
  });
  pending.set(key, request);
  return request;
}

export function BookCover({ root, file, title, format, present }: Props) {
  const target = useRef<HTMLDivElement>(null);
  const [visible, setVisible] = useState(false);
  const [cover, setCover] = useState<string | null>(null);
  useEffect(() => {
    const element = target.current;
    if (!element || !file || !["PDF", "EPUB"].includes(format.toUpperCase()) || !root) return;
    if (typeof IntersectionObserver === "undefined") { setVisible(true); return; }
    const observer = new IntersectionObserver(([entry]) => {
      if (entry.isIntersecting) { setVisible(true); observer.disconnect(); }
    }, { rootMargin: "280px" });
    observer.observe(element);
    return () => observer.disconnect();
  }, [file?.path, format, root]);
  useEffect(() => {
    let alive = true;
    if (visible && file && root) void fetchCover(root, file).then((value) => { if (alive) setCover(value); });
    return () => { alive = false; };
  }, [visible, file?.path, root]);
  return <div ref={target} className={"book-cover " + (present ? "present" : "missing") + (cover ? " has-image" : "")}>
    {cover ? <img className="book-cover-image" src={cover} alt={"Capa de " + title} draggable={false} loading="lazy" />
      : <><span className="book-cover-decoration" /><Book size={26} strokeWidth={1.35}/><span>{format}</span></>}
  </div>;
}
