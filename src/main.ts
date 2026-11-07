import "@fontsource/ibm-plex-mono/400.css";
import "@fontsource/ibm-plex-mono/400-italic.css";
import "@fontsource/ibm-plex-mono/500.css";
import "@fontsource-variable/source-serif-4/opsz.css";
import "./app.css";
import { mount } from "svelte";
import App from "./App.svelte";

// Keep the webview from acting like a browser.
window.addEventListener("contextmenu", (e) => {
  const t = e.target as HTMLElement;
  if (!t.closest("input, textarea, .cm-content")) e.preventDefault();
});
window.addEventListener("dragover", (e) => e.preventDefault());
window.addEventListener("drop", (e) => e.preventDefault());

export default mount(App, { target: document.getElementById("app")! });
