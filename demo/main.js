import { Graphviz } from "./vendor/graphviz.js";
import init, { MoineDemo } from "./pkg/moine_wasm.js";

const MAX_INPUT_CHARS = 15;

const dictionaries = {
  ja: {
    label: "Japanese",
    metadata: "./dictionaries/ja/metadata.yaml",
    payload: "./dictionaries/ja/moine-unidic-cwj-202512.readings.moineidx",
    load(demo, metadata, payload) {
      demo.loadJapaneseDictionary(metadata, payload);
    },
  },
  zh: {
    label: "Chinese",
    metadata: "./dictionaries/zh/metadata.yaml",
    payload: "./dictionaries/zh/moine-cedict-20260520.readings.moineidx",
    load(demo, metadata, payload) {
      demo.loadChineseDictionary(metadata, payload);
    },
  },
};

const els = {
  language: document.querySelector("#language"),
  left: document.querySelector("#left"),
  right: document.querySelector("#right"),
  compare: document.querySelector("#compare"),
  status: document.querySelector("#status"),
  levenshtein: document.querySelector("#levenshtein"),
  lped: document.querySelector("#lped"),
  latticePanel: document.querySelector("#lattice-panel"),
  latticeWarning: document.querySelector("#lattice-warning"),
  latticeSvg: document.querySelector("#lattice-svg"),
  examples: document.querySelectorAll("[data-lang]"),
};

let demo;
let graphviz;
const loaded = new Set();

function setStatus(message) {
  els.status.textContent = message;
}

function setResults(result) {
  els.levenshtein.value = result.levenshteinDistance;
  els.lped.value = result.latticePathEditDistance;
}

function clearResults() {
  els.levenshtein.value = "-";
  els.lped.value = "-";
  clearLatticeVisualization();
}

function clearLatticeVisualization() {
  els.latticePanel.hidden = true;
  els.latticeWarning.value = "";
  els.latticeSvg.replaceChildren();
}

function inputTooLong(value) {
  return Array.from(value).length > MAX_INPUT_CHARS;
}

function validateInputs(left, right) {
  if (inputTooLong(left) || inputTooLong(right)) {
    throw new Error(`Enter ${MAX_INPUT_CHARS} characters or fewer before comparing.`);
  }
}

async function updateLatticeVisualization(lang, left, right) {
  clearLatticeVisualization();
  const result = latticeDot(lang, left, right);
  if (!result) {
    return;
  }
  els.latticePanel.hidden = false;
  if (result.warning) {
    els.latticeWarning.value = result.warning;
    return;
  }
  if (!result.dot) {
    return;
  }

  els.latticeSvg.replaceChildren(await renderDotSvg(result.dot));
}

function latticeDot(lang, left, right) {
  if (lang === "ja") {
    return demo.japaneseLatticeDot(left, right);
  }
  if (lang === "zh") {
    return demo.chineseLatticeDot(left, right);
  }
  return null;
}

async function renderDotSvg(dot) {
  const graphviz = await ensureGraphviz();
  const svgText = graphviz.dot(dot);
  const svg = new DOMParser().parseFromString(svgText, "image/svg+xml").documentElement;
  if (svg.nodeName.toLowerCase() !== "svg") {
    throw new Error("Graphviz did not return an SVG document");
  }
  svg.setAttribute("role", "img");
  svg.setAttribute("aria-label", "Lattice graph");
  return document.importNode(svg, true);
}

async function ensureGraphviz() {
  if (!graphviz) {
    setStatus("Loading Graphviz...");
    graphviz = await Graphviz.load();
  }
  return graphviz;
}

async function fetchText(path) {
  const response = await fetch(path);
  if (!response.ok) {
    throw new Error(`${path} returned ${response.status}`);
  }
  return response.text();
}

async function fetchBytes(path) {
  const response = await fetch(path);
  if (!response.ok) {
    throw new Error(`${path} returned ${response.status}`);
  }
  return new Uint8Array(await response.arrayBuffer());
}

async function ensureDictionary(lang) {
  if (loaded.has(lang)) {
    return;
  }

  const config = dictionaries[lang];
  setStatus(`Loading ${config.label} dictionary...`);
  const [metadata, payload] = await Promise.all([
    fetchText(config.metadata),
    fetchBytes(config.payload),
  ]);
  config.load(demo, metadata, payload);
  loaded.add(lang);
}

async function compare() {
  clearResults();
  els.compare.disabled = true;
  try {
    const lang = els.language.value;
    validateInputs(els.left.value, els.right.value);
    await ensureDictionary(lang);
    const result = demo.compare(lang, els.left.value, els.right.value);
    setResults(result);
    await updateLatticeVisualization(lang, els.left.value, els.right.value);
    setStatus("");
  } catch (error) {
    setStatus(error instanceof Error ? error.message : String(error));
  } finally {
    els.compare.disabled = false;
  }
}

async function boot() {
  setStatus("Loading WASM...");
  await init();
  demo = new MoineDemo();
  setStatus("");
  els.compare.disabled = false;
}

els.compare.disabled = true;
els.compare.addEventListener("click", () => {
  void compare();
});

for (const button of els.examples) {
  button.addEventListener("click", () => {
    els.language.value = button.dataset.lang;
    els.left.value = button.dataset.left;
    els.right.value = button.dataset.right;
    void compare();
  });
}

boot().catch((error) => {
  setStatus(error instanceof Error ? error.message : String(error));
});
