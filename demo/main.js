import init, { MoineDemo } from "./pkg/moine_wasm.js";

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
  examples: document.querySelectorAll("[data-lang]"),
};

let demo;
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
    await ensureDictionary(lang);
    const result = demo.compare(lang, els.left.value, els.right.value);
    setResults(result);
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
