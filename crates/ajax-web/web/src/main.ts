import { mount } from "svelte";
import App from "./components/App.svelte";
import "./styles.css";

const target = document.getElementById("app");
if (target) {
  mount(App, { target });
} else {
  console.error("[ajax] #app element not found — Svelte app not mounted");
}
