// The browser host, exercised in a JavaScript engine (#116).
//
// CI runs this after `wasm-pack build`: it loads the bundle the dev console
// loads, gives it a world a native build made, and has the world read,
// evaluate and print — in slices, as the page does. It is not a browser: no
// DOM, no `fetch`, no timer between slices. It is the first thing that calls
// the wasm bindings at all, and the first that hands a 64-bit machine's
// image to the 32-bit one through them.
//
//   wasm-pack build crates/quokka-wasm --target web --out-dir www/pkg
//   cargo run --release -p quokka -- build crates/quokka-wasm/www/world.qi
//   node scripts/smoke-wasm.mjs
import { readFileSync } from "node:fs";
import { initSync, Quokka } from "../crates/quokka-wasm/www/pkg/quokka_wasm.js";

const www = new URL("../crates/quokka-wasm/www/", import.meta.url);
initSync({ module: readFileSync(new URL("pkg/quokka_wasm_bg.wasm", www)) });

const vm = new Quokka(new Uint8Array(readFileSync(new URL("world.qi", www))));
console.log(vm.banner());

// One line, run to its end a slice at a time, and what it printed.
const ask = (line) => {
  vm.submit(line);
  let slices = 0;
  while (vm.run(20000)) {
    if (++slices > 100000) throw new Error(`${line} did not finish`);
  }
  return vm.take_output().trim();
};

const expect = (line, wanted) => {
  const got = ask(line);
  if (got !== wanted) {
    console.error(`${line}\n  wanted ${wanted}\n  got    ${got}`);
    process.exit(1);
  }
  console.log(`${line} => ${got}`);
};

expect("(+ 1 2)", "3");
expect("(defun twice (x) (* 2 x))", "TWICE");
expect("(mapcar #'twice (list 1 2 3))", "(2 4 6)");
expect('(format nil "~A and ~S" "text" :keyword)', '"text and :KEYWORD"');
// A world saved here loads here: the image format through the bindings.
const again = new Quokka(vm.save());
again.submit("(twice 21)");
while (again.run(20000));
if (again.take_output().trim() !== "42") {
  console.error("a world saved in the engine did not come back with its definitions");
  process.exit(1);
}
console.log("a saved world came back with what was defined in it");

// The environment (phase 4, #142): the page's command loop, drawn by the
// renderer's model, which knows no DOM and so runs here. This is the gate
// in the world's and the model's halves: a form typed into the listener is
// evaluated, and an earlier form edited and run again appears below, the
// old result struck through. The model prints the screen as text.
const { Model, editEvent } = await import("../crates/quokka-wasm/www/renderer.js");
const { print, kw } = await import("../crates/quokka-wasm/www/sexp.js");

const env = new Quokka(new Uint8Array(readFileSync(new URL("world.qi", www))));
const model = new Model();
const settle = () => {
  let slices = 0;
  while (env.run(20000)) {
    if (++slices > 100000) throw new Error("the environment did not settle");
  }
  model.applyText(env.take_messages());
};
const enter = (id, text, modifiers = []) => {
  // What the view does as the person types: the page has the text first.
  model.nodes.get(id).props.text = text;
  env.post_event(editEvent(id, text, text.length, text.length));
  env.post_event(print([kw("key"), kw("target"), id, kw("key"), "Enter", kw("code"), "Enter",
                        kw("modifiers"), modifiers.map(kw), kw("start"), text.length,
                        kw("end"), text.length]));
  settle();
};
env.start("%RUN-ENVIRONMENT");
settle();
const first = model.focus.id;
enter(first, "(defun twice (x) (* 2 x))");
enter(model.focus.id, "(twice 21)");
enter(first, "(defun twice (x) (* 3 x))", ["meta"]);
enter(model.focus.id, "(twice 21)");
const screen = model.dump();
console.log(screen);
const want = `== Listener ==
CL-USER> [(defun twice (x) (* 3 x))]
~TWICE~
edited · ran again below
CL-USER> [(twice 21)]
42
CL-USER> [(defun twice (x) (* 3 x))]
↑ from 2 forms up
TWICE
CL-USER> [(twice 21)]
63
CL-USER> []
`;
if (screen !== want) {
  console.error(`the listener's screen should have read:\n${want}`);
  process.exit(1);
}
console.log("the listener evaluated a form, and an edited one again below");
