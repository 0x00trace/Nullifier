const panes = ["start", "play", "contribute", "ai-no", "ai-yes"];
function show(id) {
  for (const pane of panes) {
    const el = document.getElementById(pane);
    if (el) el.hidden = pane !== id;
  }
  const target = document.getElementById(id);
  if (target) target.scrollIntoView({ behavior: "smooth", block: "start" });
  history.replaceState(null, "", id === "start" ? location.pathname : "#" + id);
}
document.querySelectorAll("[data-view]").forEach((button) => {
  button.addEventListener("click", () => show(button.dataset.view));
});
document.querySelectorAll(".tab").forEach((tab) => {
  tab.addEventListener("click", () => {
    document.querySelectorAll(".tab").forEach((t) => t.classList.toggle("active", t === tab));
    for (const body of document.querySelectorAll(".tabbody")) {
      body.hidden = body.id !== "tab-" + tab.dataset.tab;
    }
  });
});
const initial = location.hash.replace("#", "");
if (panes.includes(initial)) show(initial);
