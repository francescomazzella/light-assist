/** Behavior that makes the pages feel like an app instead of a web page. */
export function behaveLikeAnApp() {
  // The browser's right-click menu (Back, Reload, Inspect…) has no place here. Text fields keep
  // theirs, because it is how many people paste.
  window.addEventListener("contextmenu", (event) => {
    const target = event.target;
    if (!(target instanceof HTMLInputElement || target instanceof HTMLTextAreaElement)) event.preventDefault();
  });
}
