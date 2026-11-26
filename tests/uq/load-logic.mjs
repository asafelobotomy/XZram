// Loads a page's logic.js for Node tests. The file starts `.pragma library` and makes no Qt
// calls, so it runs in a plain vm context; its top-level declarations are the API. An
// `.import "other.js" as Other` line (the page's logic split over several files) loads that file
// the same way, once, and gives it to this one as `Other`.
import { readFileSync } from "node:fs";
import vm from "node:vm";

const IMPORT = /^\s*\.import\s+"([^"]+\.js)"\s+as\s+([A-Za-z_$][\w$]*)\s*;?\s*$/;

export function loadLogic(url, cache = new Map()) {
  const key = String(url);
  if (cache.has(key)) return cache.get(key);
  const context = vm.createContext({});
  cache.set(key, context);
  const source = readFileSync(url, "utf8")
    .split("\n")
    .filter((line) => {
      const m = IMPORT.exec(line);
      if (m) context[m[2]] = loadLogic(new URL(m[1], url), cache);
      return !/^\s*\.(pragma|import)\b/.test(line);
    })
    .join("\n");
  vm.runInContext(source, context, { filename: key.split("/").pop() });
  return context;
}
