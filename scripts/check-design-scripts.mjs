// Parses the scripts of the design files the shots program loads, without
// running them, so a syntax error fails the commit hook before a shot run or a
// landing reads the file.
//
//   node scripts/check-design-scripts.mjs <file>...
//
// A board (design/mocks/*.dc.html) holds one `<script data-dc-script>`, which
// dc-runtime.js compiles with `new Function("DCLogic", "StreamableLogic",
// "React", source + ...)`. A component preview holds inline classic scripts,
// which the browser runs as they stand. design/system/components/src/*.js are
// the parts of bundle.js (design/system/build-bundle.py joins them in file-name
// order into one classic script), so a change to one parses the whole bundle.
import { readFileSync, readdirSync } from "node:fs";
import { dirname, join } from "node:path";
import vm from "node:vm";

const SCRIPT = /<script\b([^>]*)>([\s\S]*?)<\/script\s*>/gi;
const SOURCES = "design/system/components/src";

function lineOf(text, index) {
  return text.slice(0, index).split("\n").length;
}

// The line of the syntax error in `source`, found by parsing it as a script.
function errorLine(source, wrapped) {
  try {
    new vm.Script(wrapped ? `(function (DCLogic, StreamableLogic, React) {${source}\n})` : source);
  } catch (error) {
    const found = /:(\d+)\n/.exec(String(error.stack));
    if (found) return Number(found[1]);
  }
  return 1;
}

function problem(file, line, error) {
  return `${file}:${line}: ${error.message}`;
}

function parseClassic(file, source, firstLine) {
  try {
    new vm.Script(source);
  } catch (error) {
    return [problem(file, firstLine + errorLine(source, false) - 1, error)];
  }
  return [];
}

function parseLogic(file, source, firstLine) {
  try {
    new Function("DCLogic", "StreamableLogic", "React", `${source}\n;return undefined;`);
  } catch (error) {
    return [problem(file, firstLine + errorLine(source, true) - 1, error)];
  }
  return [];
}

function parseHtml(file) {
  const text = readFileSync(file, "utf8");
  const problems = [];
  for (const match of text.matchAll(SCRIPT)) {
    const attributes = match[1];
    if (/\bsrc\s*=/i.test(attributes)) continue;
    const firstLine = lineOf(text, match.index + match[0].indexOf(">") + 1);
    if (/\bdata-dc-script\b/i.test(attributes)) {
      problems.push(...parseLogic(file, match[2], firstLine));
    } else if (!/\btype\s*=/i.test(attributes)) {
      problems.push(...parseClassic(file, match[2], firstLine));
    }
  }
  return problems;
}

// bundle.js as build-bundle.py writes it, less its header and the data URIs.
function parseBundle() {
  const names = readdirSync(SOURCES).filter((name) => name.endsWith(".js")).sort();
  const parts = names.map((name) => readFileSync(join(SOURCES, name), "utf8").replace(/@@[\w.-]+@@/g, "A"));
  const source = parts.join("");
  try {
    new vm.Script(source);
  } catch (error) {
    const line = errorLine(source, false);
    let before = 0;
    for (let i = 0; i < parts.length; i += 1) {
      const lines = parts[i].split("\n").length - 1;
      if (line <= before + lines) return [problem(`${SOURCES}/${names[i]}`, line - before, error)];
      before += lines;
    }
    return [problem(SOURCES, line, error)];
  }
  return [];
}

const files = process.argv.slice(2);
const problems = [];
let bundle = false;
for (const file of files) {
  if (dirname(file) === SOURCES) {
    if (!bundle) problems.push(...parseBundle());
    bundle = true;
  } else {
    problems.push(...parseHtml(file));
  }
}
if (problems.length > 0) {
  console.error(problems.join("\n"));
  process.exit(1);
}
