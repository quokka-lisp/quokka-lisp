// The renderer (docs/display.md; phase 4 step F, #140).
//
// The world decides what is on the screen and sends messages saying so;
// this file draws what it is told and reports what the person does. It
// decides nothing: not what a key means, not where a viewer goes, not what
// colour an ink is (the theme's CSS does that). It is in two halves.
//
// The **model** applies messages to a tree of plain objects, one for each
// record the page has, found by the record's number. It knows no DOM, so
// it runs in Node, where scripts/smoke-wasm.mjs drives a real world through
// it and prints the screen as text: the "text-dump renderer" that
// docs/design.md's testing table asks for.
//
// The **view** keeps the DOM in step with the model. The model tells it
// about every change, and it changes the one element the change is about.

import { readAll, print, isSym, kw, Sym } from "./sexp.js";

// ---------------------------------------------------------------------------
// The model

// A node: { kind, id, props, children, text, parent }. A split, which is
// layout and not a record, has kind "split", a direction and parts of
// { weight, node }.
export class Model {
  constructor(observer = null) {
    this.nodes = new Map();
    this.root = null;
    this.focus = null;
    this.observer = observer;
  }

  // Apply the text of what `take_messages` returned: any number of
  // `%display` batches, each a list of messages.
  applyText(text) {
    for (const batch of readAll(text)) {
      if (!Array.isArray(batch)) continue;
      for (const message of batch) {
        try {
          this.apply(message);
        } catch (error) {
          // A message the page cannot apply is the world's mistake, or a
          // newer world's message; it is reported and skipped, and the
          // rest still apply.
          console.error("cannot apply", print(message), error);
        }
      }
    }
  }

  apply(message) {
    if (!Array.isArray(message) || !(message[0] instanceof Sym)) return;
    const [head, ...args] = message;
    switch (head.name) {
      case "reset": {
        this.nodes.clear();
        this.root = this.build(args[0], null);
        this.focus = null;
        this.notify("reset", this.root);
        break;
      }
      case "layout": {
        this.root = this.relayout(args[0]);
        this.notify("layout", this.root);
        break;
      }
      case "append":
      case "insert": {
        const parent = this.nodes.get(args[0]);
        if (!parent) return;
        const before = head.name === "insert" ? this.nodes.get(args[1]) : null;
        const node = this.build(head.name === "insert" ? args[2] : args[1], parent);
        const at = before ? parent.children.indexOf(before) : -1;
        if (at < 0) parent.children.push(node);
        else parent.children.splice(at, 0, node);
        this.notify("add", node, before && at >= 0 ? before : null);
        break;
      }
      case "replace": {
        const old = this.nodes.get(args[0]);
        if (!old || !old.parent) return;
        const node = this.build(args[1], old.parent);
        const siblings = old.parent.children;
        siblings[siblings.indexOf(old)] = node;
        this.forget(old);
        this.notify("replace", node, old);
        break;
      }
      case "remove": {
        const node = this.nodes.get(args[0]);
        if (!node) return;
        if (node.parent) {
          const siblings = node.parent.children;
          siblings.splice(siblings.indexOf(node), 1);
        }
        this.forget(node);
        this.notify("remove", node);
        break;
      }
      case "append-text": {
        // More of a text record's text: what was written since the world
        // last told the page.
        const node = this.nodes.get(args[0]);
        if (!node || node.kind !== "text" || typeof args[1] !== "string") return;
        node.text += args[1];
        this.notify("append-text", node, args[1]);
        break;
      }
      case "set": {
        const node = this.nodes.get(args[0]);
        if (!node) return;
        for (let i = 1; i + 1 < args.length; i += 2) {
          const name = args[i] instanceof Sym ? args[i].name : String(args[i]);
          const value = args[i + 1];
          if (name === "text" && node.kind === "text") node.text = value ?? "";
          else if (value === null) delete node.props[name];
          else node.props[name] = value;
          this.notify("set", node, name);
        }
        break;
      }
      case "focus": {
        const node = this.nodes.get(args[0]);
        if (!node) return;
        this.focus = node;
        this.notify("focus", node);
        break;
      }
      default:
        // A message this page does not know: from a newer world. Skipped.
        break;
    }
  }

  // A node from its S-expression, `(kind id properties child...)`, or a
  // split `(:split direction (weight layout)...)`.
  build(form, parent) {
    if (!Array.isArray(form) || !(form[0] instanceof Sym)) return null;
    const kind = form[0].name;
    if (kind === "split") {
      return {
        kind, parent,
        direction: form[1] instanceof Sym ? form[1].name : "horizontal",
        parts: form.slice(2).map(([weight, layout]) => ({
          weight: typeof weight === "number" ? weight : 1,
          node: this.build(layout, null),
        })),
      };
    }
    const node = { kind, id: form[1], props: plist(form[2]), children: [], text: "", parent };
    if (kind === "text") {
      node.text = typeof form[3] === "string" ? form[3] : "";
    } else {
      for (const child of form.slice(3)) {
        const built = this.build(child, node);
        if (built) node.children.push(built);
      }
    }
    this.nodes.set(node.id, node);
    return node;
  }

  // A layout whose viewers are known keeps their contents.
  relayout(form) {
    if (Array.isArray(form) && isSym(form[0], "viewer") && this.nodes.has(form[1])) {
      const node = this.nodes.get(form[1]);
      Object.assign(node.props, plist(form[2]));
      return node;
    }
    if (Array.isArray(form) && isSym(form[0], "split")) {
      return {
        kind: "split",
        direction: form[1] instanceof Sym ? form[1].name : "horizontal",
        parts: form.slice(2).map(([weight, layout]) => ({
          weight: typeof weight === "number" ? weight : 1,
          node: this.relayout(layout),
        })),
      };
    }
    return this.build(form, null);
  }

  // The shapes of a drawing, for the text dump: `ellipse rectangle`.
  shapes(node) {
    return node.children
      .map((c) => (c.kind === "group" ? this.shapes(c) : c.props.type ? c.props.type.name : c.kind))
      .join(" ");
  }

  forget(node) {
    this.nodes.delete(node.id);
    if (this.focus === node) this.focus = null;
    for (const child of node.children) this.forget(child);
  }

  notify(...change) {
    if (this.observer) this.observer(...change);
  }

  // The screen as text, for tests: each viewer's title, then its contents,
  // each block on lines of its own (a row's on one), an editor's text in
  // brackets, and what is struck out in tildes.
  dump(node = this.root, inRow = false) {
    if (!node) return "";
    const struck = (text) => (node.props && node.props.strike ? `~${text}~` : text);
    switch (node.kind) {
      case "split":
        return node.parts.map((p) => this.dump(p.node)).join("\n");
      case "viewer":
        return `== ${node.props.title ?? ""} ==\n` + node.children.map((c) => this.dump(c)).join("");
      case "text":
        return struck(node.text);
      case "editor":
        return `[${node.props.text ?? ""}]`;
      case "drawing":
        return `<drawing ${node.props.width}x${node.props.height}: ${this.shapes(node)}>\n`;
      case "block": {
        const row = node.props.layout instanceof Sym && node.props.layout.name === "row";
        const inner = node.children.map((c) => this.dump(c, row)).join("");
        if (inRow || inner === "") return struck(inner);
        const line = inner.endsWith("\n") ? inner.slice(0, -1) : inner;
        return (line === "" ? "" : struck(line)) + "\n";
      }
      default:
        return node.children.map((c) => this.dump(c, inRow)).join("");
    }
  }
}

// A property list as an object, keys in lower case.
function plist(list) {
  const out = {};
  if (!Array.isArray(list)) return out;
  for (let i = 0; i + 1 < list.length; i += 2) {
    const key = list[i] instanceof Sym ? list[i].name : String(list[i]);
    out[key] = list[i + 1];
  }
  return out;
}

// ---------------------------------------------------------------------------
// Events, as the world reads them

export const editEvent = (id, text, start, end) =>
  print([kw("edit"), kw("target"), id, kw("text"), text, kw("start"), start, kw("end"), end]);

export function keyEvent(id, event, area = null) {
  const modifiers = [];
  if (event.shiftKey) modifiers.push(kw("shift"));
  if (event.ctrlKey) modifiers.push(kw("control"));
  if (event.altKey) modifiers.push(kw("alt"));
  if (event.metaKey) modifiers.push(kw("meta"));
  const where = area ? [kw("start"), area.selectionStart, kw("end"), area.selectionEnd] : [];
  return print([kw("key"), kw("target"), id, kw("key"), event.key, kw("code"), event.code,
                kw("modifiers"), modifiers, kw("repeat"), !!event.repeat, ...where]);
}

// The name a key table uses for a key: "Enter", "Shift+Enter", "Meta+Enter".
export function keyName(event) {
  let name = "";
  if (event.ctrlKey) name += "Control+";
  if (event.altKey) name += "Alt+";
  if (event.shiftKey) name += "Shift+";
  if (event.metaKey) name += "Meta+";
  return name + event.key;
}

// ---------------------------------------------------------------------------
// The view

export class View {
  // ROOT is the element the surface fills; POST sends an event's text to
  // the world.
  constructor(root, post) {
    this.root = root;
    this.post = post;
    this.model = new Model((...change) => this.change(...change));
  }

  change(what, node, other) {
    switch (what) {
      case "reset":
      case "layout":
        this.root.replaceChildren(this.element(node));
        break;
      case "add": {
        const parent = this.contents(node.parent);
        if (!parent) return;
        const element = this.element(node);
        if (other && other.dom && other.dom.parentNode === parent) parent.insertBefore(element, other.dom);
        else parent.append(element);
        break;
      }
      case "replace":
        if (other.dom) other.dom.replaceWith(this.element(node));
        break;
      case "remove":
        if (node.dom) node.dom.remove();
        break;
      case "append-text":
        // The element holds the text and nothing else, so the new
        // characters are added to it; its look is in its classes.
        if (node.dom) node.dom.append(other);
        break;
      case "set":
        this.update(node, other);
        break;
      case "focus":
        if (node.dom && node.kind === "editor") {
          node.dom.focus();
          const end = node.dom.value.length;
          node.dom.setSelectionRange(end, end);
        } else if (node.dom) {
          node.dom.scrollIntoView({ block: "nearest" });
        }
        break;
    }
  }

  // Where a node's children go.
  contents(node) {
    if (node && node.dom && (node.kind === "drawing" || node.kind === "group")) return node.dom;
    if (!node || !node.dom) return null;
    return node.kind === "viewer" ? node.dom.querySelector(":scope > .q-contents") : node.dom;
  }

  element(node) {
    if (!node) return document.createElement("div");
    if (node.kind === "split") {
      const div = document.createElement("div");
      div.className = `q-split q-${node.direction}`;
      for (const part of node.parts) {
        const child = this.element(part.node);
        child.style.flex = `${part.weight} 1 0`;
        div.append(child);
      }
      return div;
    }
    if (node.dom && node.kind === "viewer" && node.dom.isConnected === false && node.children.length) {
      // A viewer the layout moved keeps its element, and so its contents.
      return node.dom;
    }
    let element;
    switch (node.kind) {
      case "viewer": {
        element = document.createElement("section");
        element.className = "q-viewer";
        const title = document.createElement("header");
        title.className = "q-title";
        const contents = document.createElement("div");
        contents.className = "q-contents";
        element.append(title, contents);
        break;
      }
      case "block":
        element = document.createElement("div");
        break;
      case "text":
        element = document.createElement("span");
        break;
      case "editor":
        element = this.editor(node);
        break;
      case "drawing":
        element = document.createElementNS(SVG, "svg");
        break;
      case "group":
        element = document.createElementNS(SVG, "g");
        break;
      case "shape":
        element = document.createElementNS(SVG, SHAPES[node.props.type?.name] ?? "g");
        break;
      default:
        // A kind this page does not know: its children, plainly.
        element = document.createElement("span");
    }
    node.dom = element;
    element.dataset.id = node.id;
    this.style(node);
    if (node.kind === "text") element.textContent = node.text;
    const contents = this.contents(node);
    if (contents && node.kind !== "editor") {
      for (const child of node.children) contents.append(this.element(child));
    }
    return element;
  }

  editor(node) {
    const area = document.createElement("textarea");
    area.spellcheck = false;
    area.rows = 1;
    area.value = node.props.text ?? "";
    area.addEventListener("input", () => {
      node.props.text = area.value;
      this.fit(area);
      this.post(editEvent(node.id, area.value, area.selectionStart, area.selectionEnd));
    });
    area.addEventListener("keydown", (event) => {
      const keys = node.props.keys;
      // An Enter that confirms an input method's composition is the
      // browser's, as the rest of input methods are (decision 5).
      if (event.isComposing || event.keyCode === 229) return;
      if (Array.isArray(keys) && keys.includes(keyName(event))) {
        event.preventDefault();
        this.post(keyEvent(node.id, event, area));
      }
    });
    queueMicrotask(() => this.fit(area));
    return area;
  }

  // An editor is as tall as its text.
  fit(area) {
    area.style.height = "auto";
    area.style.height = `${area.scrollHeight}px`;
  }

  update(node, name) {
    const element = node.dom;
    if (!element) return;
    if (node.kind === "text" && name === "text") {
      element.textContent = node.text;
    } else if (node.kind === "editor" && (name === "text" || name === "start" || name === "end")) {
      const text = node.props.text ?? "";
      if (element.value !== text) {
        element.value = text;
        this.fit(element);
      }
      if (typeof node.props.start === "number" && document.activeElement === element) {
        element.setSelectionRange(node.props.start, node.props.end ?? node.props.start);
      }
    } else if (node.kind === "viewer" && name === "title") {
      element.querySelector(":scope > .q-title").textContent = node.props.title ?? "";
    } else {
      this.style(node);
    }
  }

  // A picture's box, a group's transformation, a shape's geometry: SVG's
  // own attributes, which take the world's numbers as they are
  // (docs/design.md, *Coordinates*).
  geometry(node) {
    const e = node.dom;
    const p = node.props;
    const set = (name, value) => (value === undefined || value === null
      ? e.removeAttribute(name) : e.setAttribute(name, String(value)));
    if (node.kind === "drawing") {
      set("viewBox", `0 0 ${p.width} ${p.height}`);
      const scale = p.scale instanceof Sym ? p.scale.name : null;
      set("preserveAspectRatio", scale === "stretch" ? "none" : "xMidYMid meet");
      set("width", scale ? null : p.width);
      set("height", scale ? null : p.height);
      e.setAttribute("class", `q-drawing q-drawing-${scale ?? "fixed"}`);
      if (scale === "fit") e.style.aspectRatio = `${p.width} / ${p.height}`;
      return;
    }
    if (node.kind === "group") {
      set("transform", Array.isArray(p.transform) ? `matrix(${p.transform.join(" ")})` : null);
      return;
    }
    const points = Array.isArray(p.points) ? p.points : [];
    switch (p.type?.name) {
      case "rectangle":
        set("x", p.x); set("y", p.y); set("width", p.width); set("height", p.height);
        set("rx", p["corner-radius"]);
        break;
      case "ellipse":
        set("cx", p.cx); set("cy", p.cy); set("rx", p.rx); set("ry", p.ry);
        break;
      case "line":
        set("x1", points[0]); set("y1", points[1]); set("x2", points[2]); set("y2", points[3]);
        break;
      case "polyline":
      case "polygon": {
        const pairs = [];
        for (let i = 0; i + 1 < points.length; i += 2) pairs.push(`${points[i]},${points[i + 1]}`);
        set("points", pairs.join(" "));
        break;
      }
      case "text":
        set("x", p.x); set("y", p.y);
        set("text-anchor", { center: "middle", end: "end" }[p.align?.name] ?? "start");
        e.textContent = p.text ?? "";
        break;
    }
    set("stroke-width", p["line-thickness"]);
  }

  // Classes from a node's style and ink, never CSS from the world.
  style(node) {
    const element = node.dom;
    if (node.kind === "drawing" || node.kind === "group" || node.kind === "shape") {
      return this.shapeStyle(node);
    }
    const classes = [`q-${node.kind}`];
    const style = node.props.style;
    if (Array.isArray(style)) {
      const [family, face, size] = style;
      if (family instanceof Sym) classes.push(`q-family-${family.name}`);
      if (face instanceof Sym) classes.push(`q-face-${face.name}`);
      if (size instanceof Sym) classes.push(`q-size-${size.name}`);
    }
    if (node.props.ink instanceof Sym) classes.push(`q-ink-${node.props.ink.name}`);
    if (node.props.layout instanceof Sym) classes.push(`q-layout-${node.props.layout.name}`);
    if (node.props.strike) classes.push("q-strike");
    if (node.props.note) classes.push("q-note");
    if (node.props.bar) classes.push("q-bar");
    if (node.kind === "editor") element.readOnly = !!node.props.readonly;
    if (node.kind === "editor") classes.push("q-editor");
    element.className = classes.join(" ");
    if (node.kind === "viewer") {
      element.querySelector(":scope > .q-title").textContent = node.props.title ?? "";
    }
  }

  // A shape is filled with its ink, or, unfilled or a line, drawn in it.
  shapeStyle(node) {
    const element = node.dom;
    const p = node.props;
    const ink = p.ink instanceof Sym ? p.ink.name : "ink";
    const classes = [`q-${node.kind}`];
    if (node.kind === "shape") {
      const type = p.type?.name;
      const line = type === "line" || type === "polyline";
      classes.push(!line && p.filled ? `q-fill-${ink}` : `q-stroke-${ink}`);
      if (type === "text") classes.push("q-drawn-text");
      const style = p.style;
      if (Array.isArray(style)) {
        const [family, face, size] = style;
        if (family instanceof Sym) classes.push(`q-family-${family.name}`);
        if (face instanceof Sym) classes.push(`q-face-${face.name}`);
        if (size instanceof Sym) classes.push(`q-size-${size.name}`);
      }
    }
    element.setAttribute("class", classes.join(" "));
    this.geometry(node);
  }

  apply(text) {
    this.model.applyText(text);
  }
}

const SVG = "http://www.w3.org/2000/svg";
const SHAPES = {
  rectangle: "rect", ellipse: "ellipse", line: "line", polyline: "polyline",
  polygon: "polygon", text: "text",
};
