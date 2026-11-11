// Loads a page's logic.js for Node tests. The file starts `.pragma library` and makes no Qt
// calls, so it runs in a plain vm context; its top-level declarations are the API.
import { readFileSync } from "node:fs";
import vm from "node:vm";

export function loadLogic(url) {
  const source = readFileSync(url, "utf8")
    .split("\n")
    .filter((line) => !/^\s*\.(pragma|import)\b/.test(line))
    .join("\n");
  const context = vm.createContext({});
  vm.runInContext(source, context, { filename: "logic.js" });
  return context;
}
