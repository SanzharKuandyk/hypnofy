export type Theme = "dark" | "light" | "system";

const system = matchMedia("(prefers-color-scheme: dark)");
let preference: Theme = "dark";

export function applyTheme(theme: Theme): void {
  preference = theme;
  document.documentElement.dataset.theme =
    theme === "system" ? (system.matches ? "dark" : "light") : theme;
  localStorage.setItem("hypnofy.theme", theme);
}

export function initializeTheme(): void {
  const stored = localStorage.getItem("hypnofy.theme");
  applyTheme(stored === "light" || stored === "system" ? stored : "dark");
}

system.addEventListener("change", () => {
  if (preference === "system") applyTheme("system");
});
