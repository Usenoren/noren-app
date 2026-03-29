import { getSettings, setTheme } from "$lib/api/tauri";

export type PaletteId =
  | "kon"
  | "charcoal"
  | "classic"
  | "sumi"
  | "washi"
  | "matcha"
  | "kumo"
  | "yoru";

export interface PaletteInfo {
  id: PaletteId;
  name: string;
  vibe: string;
  isLight: boolean;
  bg: string;
  surface: string;
  border: string;
  accent: string;
  sendColor: string;
}

export const PALETTES: PaletteInfo[] = [
  {
    id: "kon",
    name: "Kon",
    vibe: "Navy",
    isLight: false,
    bg: "#1E3148",
    surface: "#253A52",
    border: "#2A4560",
    accent: "#7A3340",
    sendColor: "#7A3340",
  },
  {
    id: "charcoal",
    name: "Charcoal",
    vibe: "Warm dark",
    isLight: false,
    bg: "#141211",
    surface: "#1E1B19",
    border: "#302C28",
    accent: "#7A3340",
    sendColor: "#7A3340",
  },
  {
    id: "classic",
    name: "Classic",
    vibe: "Warm cream",
    isLight: true,
    bg: "#F6F1EB",
    surface: "#FFFDF9",
    border: "#D4CFC9",
    accent: "#7A3340",
    sendColor: "#7A3340",
  },
  {
    id: "sumi",
    name: "Sumi",
    vibe: "Ink black",
    isLight: false,
    bg: "#0C0B0A",
    surface: "#141312",
    border: "#222120",
    accent: "#7A3340",
    sendColor: "#7A3340",
  },
  {
    id: "washi",
    name: "Washi",
    vibe: "Paper white",
    isLight: true,
    bg: "#FAFAF7",
    surface: "#FFFFFF",
    border: "#E5E2DD",
    accent: "#7A3340",
    sendColor: "#7A3340",
  },
  {
    id: "matcha",
    name: "Matcha",
    vibe: "Deep forest",
    isLight: false,
    bg: "#141E17",
    surface: "#1A2820",
    border: "#263830",
    accent: "#7A3340",
    sendColor: "#5A7A60",
  },
  {
    id: "kumo",
    name: "Kumo",
    vibe: "Cloud gray",
    isLight: true,
    bg: "#E8E6E2",
    surface: "#F2F0ED",
    border: "#D0CEC9",
    accent: "#7A3340",
    sendColor: "#7A3340",
  },
  {
    id: "yoru",
    name: "Yoru",
    vibe: "Midnight",
    isLight: false,
    bg: "#0D1117",
    surface: "#151B24",
    border: "#21283B",
    accent: "#7A3340",
    sendColor: "#7A3340",
  },
];

const LIGHT_THEMES: PaletteId[] = ["classic", "washi", "kumo"];
const DEFAULT_THEME: PaletteId = "kon";
const LS_KEY = "noren:theme";

const VALID_IDS = new Set<string>(PALETTES.map((p) => p.id));

let currentTheme = $state<PaletteId>(DEFAULT_THEME);

export function getTheme(): PaletteId {
  return currentTheme;
}

export function applyTheme(id: PaletteId): void {
  if (!VALID_IDS.has(id)) id = DEFAULT_THEME;
  currentTheme = id;
  document.documentElement.setAttribute("data-theme", id);
  document.documentElement.setAttribute(
    "data-theme-mode",
    LIGHT_THEMES.includes(id) ? "light" : "dark",
  );
  localStorage.setItem(LS_KEY, id);
}

export function initTheme(): void {
  const cached = localStorage.getItem(LS_KEY) as PaletteId | null;
  applyTheme(cached || DEFAULT_THEME);
}

export async function setAndPersistTheme(id: PaletteId): Promise<void> {
  applyTheme(id);
  await setTheme(id);
}

export async function syncThemeFromConfig(): Promise<void> {
  try {
    const settings = await getSettings();
    if (settings.theme && VALID_IDS.has(settings.theme)) {
      applyTheme(settings.theme as PaletteId);
    }
  } catch {}
}
