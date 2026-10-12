import { createApp } from "vue";
import App from "./App.vue";
import { behaveLikeAnApp } from "./app-chrome.js";
import "./style.css";

behaveLikeAnApp();
createApp(App).mount("#app");