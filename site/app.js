const panes = ["start", "play", "contribute"];
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
const noAi = document.getElementById("no-ai");
const yes = document.getElementById("have-yes");
const no = document.getElementById("have-no");
if (yes && no && noAi) {
  yes.addEventListener("click", () => {
    noAi.hidden = true;
    yes.classList.add("chosen");
    no.classList.remove("chosen");
  });
  no.addEventListener("click", () => {
    noAi.hidden = false;
    no.classList.add("chosen");
    yes.classList.remove("chosen");
  });
}
document.querySelectorAll("[data-copy]").forEach((button) => {
  button.addEventListener("click", async () => {
    const el = document.getElementById(button.dataset.copy);
    if (!el) return;
    const text = el.innerText;
    try {
      await navigator.clipboard.writeText(text);
      button.textContent = "Copied";
      setTimeout(() => (button.textContent = "Copy"), 1500);
    } catch {
      const range = document.createRange();
      range.selectNodeContents(el);
      getSelection().removeAllRanges();
      getSelection().addRange(range);
      document.execCommand("copy");
      button.textContent = "Copied";
    }
  });
});
const initial = location.hash.replace("#", "");
if (panes.includes(initial)) show(initial);
