import { createApp } from "vue";
import HelperPage from "./views/HelperPage.vue";
import { behaveLikeAnApp } from "./app-chrome.js";
import { connect, phase } from "./helper-session.js";
import { stayUpToDate } from "./hosted.js";
import { watch } from "vue";
import "./style.css";

behaveLikeAnApp();
createApp(HelperPage).mount("#app");
connect();

// Nothing is lost by loading a newer page while nobody is reading a code out or in a session.
const reloadIfIdle = stayUpToDate(() => phase.value !== "code" && phase.value !== "admitted");
if (reloadIfIdle) watch(phase, reloadIfIdle);
