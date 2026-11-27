// S-expressions, as the display protocol carries them (docs/display.md,
// decision 1): lists, strings, integers, floats, keywords, nil and t. The
// world prints messages with its own printer under the standard syntax, and
// reads events with its own reader; this is the page's half of both.
//
// A list is a JavaScript array, a string a string, a number a number, nil
// null and t true. A keyword or other symbol is a `Sym`, whose name is kept
// in lower case: `:APPEND` is `new Sym("append")`, compared by `isSym`.

export class Sym {
  constructor(name) { this.name = name; }
  toString() { return ":" + this.name; }
}

export const isSym = (value, name) => value instanceof Sym && value.name === name;

// Every form in TEXT, in order.
export function readAll(text) {
  let at = 0;
  const forms = [];

  const skip = () => {
    for (;;) {
      while (at < text.length && /\s/.test(text[at])) at++;
      if (text[at] === ";") {
        while (at < text.length && text[at] !== "\n") at++;
      } else {
        return;
      }
    }
  };

  const read = () => {
    skip();
    if (at >= text.length) throw new Error("end of input inside a form");
    const c = text[at];
    if (c === "(") {
      at++;
      const list = [];
      for (;;) {
        skip();
        if (text[at] === ")") { at++; return list; }
        if (at >= text.length) throw new Error("end of input inside a list");
        list.push(read());
      }
    }
    if (c === ")") throw new Error("unexpected )");
    if (c === '"') {
      at++;
      let out = "";
      while (at < text.length && text[at] !== '"') {
        if (text[at] === "\\") at++;
        out += text[at++];
      }
      at++;
      return out;
    }
    let token = "";
    while (at < text.length && !/[\s()";]/.test(text[at])) token += text[at++];
    return atom(token);
  };

  for (;;) {
    skip();
    if (at >= text.length) return forms;
    forms.push(read());
  }
}

// A token: a number, nil, t, or a symbol. The world prints doubles with a
// `d0` exponent under the standard syntax (`1.5d0`), which JavaScript reads
// as `e`.
function atom(token) {
  if (/^[+-]?\d+\.?$/.test(token)) return parseInt(token, 10);
  if (/^[+-]?(\d+\.?\d*|\.\d+)([eEdDfFsSlL][+-]?\d+)?$/.test(token)) {
    return parseFloat(token.replace(/[dDfFsSlL]/, "e"));
  }
  const upper = token.toUpperCase();
  if (upper === "NIL") return null;
  if (upper === "T") return true;
  return new Sym(token.replace(/^:/, "").toLowerCase());
}

// VALUE printed for the world's reader.
export function print(value) {
  if (value === null || value === undefined || value === false) return "nil";
  if (value === true) return "t";
  if (Array.isArray(value)) return "(" + value.map(print).join(" ") + ")";
  if (value instanceof Sym) return ":" + value.name;
  if (typeof value === "number") {
    return Number.isInteger(value) ? String(value) : String(value).replace(/e/, "d") +
      (String(value).includes("e") ? "" : "d0");
  }
  return '"' + String(value).replace(/\\/g, "\\\\").replace(/"/g, '\\"') + '"';
}

// A keyword, for building events: kw("edit").
export const kw = (name) => new Sym(name);
