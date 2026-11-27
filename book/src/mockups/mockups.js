// A mock-up is a page with numbered steps. Each later step replaces the
// contents of a few named regions; step 1 is what the HTML itself holds,
// so the page reads correctly without JavaScript. The strings are the
// page's own fixed text, never input. Open a page at #step=N to start on
// step N (render-mockups.sh uses this to make the PNGs), and add &figure
// to show only the screen, for a tutorial's illustrations.
function mockupSteps(later) {
  const ids = new Set(later.flatMap(Object.keys));
  const first = {};
  for (const id of ids) first[id] = document.getElementById(id).innerHTML;
  const steps = [first, ...later];
  const buttons = document.querySelectorAll(".steps button");
  function go(i) {
    for (const id of ids) {
      if (id in steps[i]) document.getElementById(id).innerHTML = steps[i][id];
      else document.getElementById(id).innerHTML = first[id];
    }
    buttons.forEach((b, j) => b.classList.toggle("on", j === i));
  }
  buttons.forEach((b, j) => b.addEventListener("click", () => go(j)));
  if (/figure/.test(location.hash)) document.body.classList.add("figure");
  const m = location.hash.match(/step=(\d+)/);
  go(m ? Math.min(steps.length, Math.max(1, Number(m[1]))) - 1 : 0);
}
