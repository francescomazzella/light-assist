import { createApp } from "vue";
import { behaveLikeAnApp } from "./app-chrome.js";
import Indicator from "./views/Indicator.vue";
import "./style.css";

behaveLikeAnApp();
createApp(Indicator).mount("#app");
