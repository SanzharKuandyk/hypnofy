import "@fontsource/inter/400.css";
import "@fontsource/inter/500.css";
import "@fontsource/inter/600.css";
import "@fontsource/lora/400.css";
import "./shared/theme/tokens.css";
import App from "./app/App.svelte";
import { mount } from "svelte";
import { initializeTheme } from "./shared/theme/theme";

initializeTheme();
const target = document.getElementById("app");
if (!target) throw new Error("Application mount is missing");
const Root =
  import.meta.env.DEV && new URLSearchParams(location.search).has("showcase")
    ? (await import("./shared/testing/Showcase.svelte")).default
    : App;
const app = mount(Root, { target });
export default app;
