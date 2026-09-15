const ZED_FONTS = [
  "Times New Roman",
  "Georgia",
  "PT Serif",
  "Arial",
  "Calibri",
  "PT Sans",
  "Segoe UI",
  "Courier New",
];
const ZED_SIZES = [10, 11, 12, 14, 16, 18, 20, 24, 28, 36];

function uid() {
  return "b" + Math.random().toString(36).slice(2, 10);
}

function parseLayout(body) {
  const raw = (body || "").trim();
  if (raw.startsWith("{")) {
    try {
      const j = JSON.parse(raw);
      if (j && Array.isArray(j.blocks)) {
        j.blocks = j.blocks.map(normalizeBlock);
        j.pages = Math.min(5, Math.max(1, Number(j.pages || 1)));
        return j;
      }
    } catch { /* plain */ }
  }
  const lines = raw ? raw.split(/\n/) : [""];
  return { v: 1, pages: 1, blocks: lines.map((line) => normalizeBlock({ html: line })) };
}

function normalizeBlock(b) {
  const base = {
    id: (b && b.id) || uid(),
    type: "block",
    align: (b && b.align) || "left",
    font: (b && b.font) || "Times New Roman",
    size: (b && b.size) || 14,
    indent: Number((b && b.indent) || 0),
  };
  if (b && b.type === "header") {
    return { ...base, cols: 2, html: [b.left || "", b.right || ""] };
  }
  let html = [];
  if (b && Array.isArray(b.html)) html = b.html.slice();
  else if (b && typeof b.html === "string") html = [b.html];
  else if (b && (b.left != null || b.right != null)) html = [b.left || "", b.right || ""];
  else html = [""];
  let cols = Number((b && b.cols) || html.length || 1);
  cols = Math.min(4, Math.max(1, cols));
  while (html.length < cols) html.push("");
  html = html.slice(0, cols);
  return { ...base, cols, html };
}

function hydrateHtml(html, fields) {
  const s = html || "";
  const re = /\{\{([a-z][a-z0-9_]*)\}\}/g;
  let out = "";
  let last = 0;
  let m;
  while ((m = re.exec(s))) {
    out += s.slice(last, m.index);
    const key = m[1];
    const f = (fields || []).find((x) => x.key === key);
    out += `<span class="chip" data-key="${esc(key)}" contenteditable="false">${esc(f ? f.label : key)}</span>\u200B`;
    last = m.index + m[0].length;
  }
  return out + s.slice(last);
}

function dehydrateEl(el) {
  const clone = el.cloneNode(true);
  clone.querySelectorAll(".chip").forEach((chip) => {
    chip.replaceWith(document.createTextNode(`{{${chip.dataset.key}}}`));
  });
  return clone.innerHTML.replace(/\u200B/g, "");
}

function harvestLayout(page) {
  const blocks = [];
  page.querySelectorAll(".zed-block").forEach((node) => {
    const font = node.dataset.font || "Times New Roman";
    const size = Number(node.dataset.size || 14);
    const cells = [...node.querySelectorAll(".zed-edit")].map((el) => dehydrateEl(el));
    const cols = Math.min(4, Math.max(1, Number(node.dataset.cols || cells.length || 1)));
    while (cells.length < cols) cells.push("");
    blocks.push({
      id: node.dataset.id,
      type: "block",
      cols,
      align: node.dataset.align || "left",
      font,
      size,
      indent: Number(node.dataset.indent || 0),
      html: cells.slice(0, cols),
    });
  });
  const pages = Math.min(5, Math.max(1, Number(document.getElementById("zed-pages")?.value || 1)));
  return { v: 1, pages, blocks };
}

function blockStyle(b) {
  const font = b.font || "Times New Roman";
  const size = b.size || 14;
  const align = b.align || "left";
  const indent = Number(b.indent || 0);
  const ind = indent > 0 ? `text-indent:${indent * 1.25}cm;` : "";
  return `font-family:'${font}',Times,serif;font-size:${size}pt;text-align:${align};${ind}`;
}

function renderBlock(b, fields) {
  const nb = normalizeBlock(b);
  const bar = `<div class="zed-block-bar">
    <button type="button" class="ghost compact" data-up title="выше">↑</button>
    <button type="button" class="ghost compact" data-down title="ниже">↓</button>
    <span class="muted">блок</span>
    ${[1, 2, 3, 4].map((n) => `<button type="button" class="ghost compact ${nb.cols === n ? "on" : ""}" data-cols="${n}" title="${n} колонк${n === 1 ? "а" : "и"}">${n}</button>`).join("")}
    <button type="button" class="ghost compact" data-rm>убрать</button>
  </div>`;
  const cells = nb.html.map((cell, i) => {
    const ta = nb.cols === 2 && i === 1 ? "right" : (nb.align || "left");
    const style = blockStyle({ ...nb, align: ta, indent: nb.cols === 1 ? nb.indent : 0 });
    return `<div class="zed-edit" data-col="${i}" contenteditable="true" spellcheck="false" style="${style}">${hydrateHtml(cell || "", fields)}</div>`;
  }).join("");
  return `<div class="zed-block" data-id="${esc(nb.id)}" data-type="block" data-cols="${nb.cols}" data-align="${esc(nb.align)}" data-font="${esc(nb.font)}" data-size="${nb.size}" data-indent="${nb.indent}">
    ${bar}
    <div class="zed-cols zed-cols-${nb.cols}">${cells}</div>
  </div>`;
}

function toolbarHtml() {
  return `<div class="zed-toolbar" id="zed-tb">
    <select id="zed-font">${ZED_FONTS.map((f) => `<option value="${esc(f)}">${esc(f)}</option>`).join("")}</select>
    <select id="zed-size">${ZED_SIZES.map((s) => `<option value="${s}" ${s === 14 ? "selected" : ""}>${s}</option>`).join("")}</select>
    <button type="button" class="ghost compact" data-cmd="bold"><b>Ж</b></button>
    <button type="button" class="ghost compact" data-cmd="italic"><i>К</i></button>
    <button type="button" class="ghost compact" data-cmd="underline"><u>Ч</u></button>
    <button type="button" class="ghost compact" data-align="left" title="к левому краю">⬅</button>
    <button type="button" class="ghost compact" data-align="center" title="по центру">↔</button>
    <button type="button" class="ghost compact" data-align="right" title="к правому краю">➡</button>
    <select id="zed-indent" title="отступ первой строки">
      <option value="0">без отступа</option>
      <option value="1">отступ (Tab)</option>
      <option value="2">двойной Tab</option>
    </select>
    <span class="zed-tb-gap"></span>
    <select id="zed-pages" title="сколько листов A4">
      <option value="1">1 лист</option>
      <option value="2">2 листа</option>
      <option value="3">3 листа</option>
    </select>
    <button type="button" class="ghost compact" id="zed-add-p">+ блок</button>
    <select id="zed-place-field" title="вставить существующее поле">
      <option value="">Вставить поле…</option>
    </select>
  </div>`;
}

function focusedEdit() {
  const ae = document.activeElement;
  if (ae && ae.classList.contains("zed-edit")) return ae;
  return window._zedLastEdit || null;
}

function setIndent(block, n) {
  block.dataset.indent = String(n);
  const edit = block.querySelector(".zed-edit");
  if (edit) edit.style.textIndent = n > 0 ? `${n * 1.25}cm` : "0";
  const sel = document.getElementById("zed-indent");
  if (sel) sel.value = String(Math.min(2, n));
}

function applyFontSize(pt) {
  const edit = focusedEdit();
  if (!edit) return;
  edit.focus();
  document.execCommand("styleWithCSS", false, true);
  document.execCommand("fontSize", false, "7");
  edit.querySelectorAll('font[size="7"], span[style*="xxx-large"]').forEach((el) => {
    const span = document.createElement("span");
    span.style.fontSize = `${pt}pt`;
    while (el.firstChild) span.appendChild(el.firstChild);
    el.replaceWith(span);
  });
  const block = edit.closest(".zed-block");
  if (block && (!window.getSelection() || window.getSelection().isCollapsed)) {
    block.dataset.size = String(pt);
    edit.style.fontSize = `${pt}pt`;
  }
}

function bindBlockEditor(page, layout, ctx) {
  const setCols = (node, cols) => {
    const one = harvestLayout(page).blocks.find((b) => b.id === node.dataset.id);
    if (!one) return;
    cols = Math.min(4, Math.max(1, Number(cols)));
    let html = (one.html || []).slice();
    while (html.length < cols) html.push("");
    if (html.length > cols) {
      const rest = html.slice(cols).filter(Boolean).join(" ");
      html = html.slice(0, cols);
      if (rest) html[html.length - 1] = `${html[html.length - 1]} ${rest}`.trim();
    }
    const wrap = document.createElement("div");
    wrap.innerHTML = renderBlock({ ...one, cols, html }, ctx.fields());
    node.replaceWith(wrap.firstElementChild);
  };

  page.addEventListener("focusin", (e) => {
    if (e.target.classList.contains("zed-edit")) window._zedLastEdit = e.target;
  });
  page.addEventListener("keydown", (e) => {
    if ((e.key === "Backspace" || e.key === "Delete") && e.target.classList.contains("zed-edit")) {
      const on = page.querySelector(".chip.chip-on");
      if (on) {
        e.preventDefault();
        on.remove();
        return;
      }
      const chip = adjacentChip(e.target, e.key === "Backspace" ? "back" : "del");
      if (chip) {
        e.preventDefault();
        chip.remove();
        return;
      }
    }
    if (e.key !== "Tab" || !e.target.classList.contains("zed-edit")) return;
    const block = e.target.closest(".zed-block");
    if (!block || Number(block.dataset.cols || 1) !== 1) return;
    e.preventDefault();
    let n = Number(block.dataset.indent || 0);
    n = e.shiftKey ? Math.max(0, n - 1) : Math.min(4, n + 1);
    setIndent(block, n);
  });
  page.addEventListener("mousedown", (e) => {
    const chip = e.target.closest(".chip");
    page.querySelectorAll(".chip.chip-on").forEach((c) => c.classList.remove("chip-on"));
    if (chip && page.contains(chip)) {
      chip.classList.add("chip-on");
    }
  });
  page.addEventListener("click", (e) => {
    const node = e.target.closest(".zed-block");
    if (!node || !page.contains(node)) return;
    if (e.target.closest(".zed-block-bar [data-cols]")) {
      setCols(node, e.target.closest(".zed-block-bar [data-cols]").dataset.cols);
      return;
    }
    if (e.target.closest(".zed-block-bar [data-up]")) {
      if (node.previousElementSibling) node.parentNode.insertBefore(node, node.previousElementSibling);
      return;
    }
    if (e.target.closest(".zed-block-bar [data-down]")) {
      if (node.nextElementSibling) node.parentNode.insertBefore(node.nextElementSibling, node);
      return;
    }
    if (e.target.closest(".zed-block-bar [data-rm]")) {
      node.remove();
      if (!page.querySelector(".zed-block")) {
        layout.blocks = [normalizeBlock({ cols: 1, html: [""] })];
        page.innerHTML = renderBlock(layout.blocks[0], ctx.fields());
      }
    }
  });

  const tb = document.getElementById("zed-tb");
  tb.addEventListener("mousedown", (e) => {
    if (e.target.closest("button")) e.preventDefault();
  });

  document.querySelectorAll("#zed-tb [data-cmd]").forEach((btn) => {
    btn.addEventListener("click", () => {
      const edit = focusedEdit();
      if (edit) edit.focus();
      document.execCommand("styleWithCSS", false, true);
      document.execCommand(btn.dataset.cmd, false, null);
      styleChips(page, btn.dataset.cmd);
    });
  });

  document.querySelectorAll("#zed-tb [data-align]").forEach((btn) => {
    btn.addEventListener("click", () => {
      const edit = focusedEdit();
      if (!edit) return;
      const block = edit.closest(".zed-block");
      if (Number(block.dataset.cols || 1) === 1) {
        block.dataset.align = btn.dataset.align;
      }
      edit.style.textAlign = btn.dataset.align;
    });
  });

  document.getElementById("zed-font").addEventListener("change", (e) => {
    const edit = focusedEdit();
    if (edit) edit.focus();
    document.execCommand("fontName", false, e.target.value);
    const block = focusedEdit()?.closest(".zed-block");
    if (block) {
      block.dataset.font = e.target.value;
      block.querySelectorAll(".zed-edit").forEach((el) => { el.style.fontFamily = `'${e.target.value}', Times, serif`; });
    }
  });

  document.getElementById("zed-size").addEventListener("change", (e) => {
    applyFontSize(Number(e.target.value));
  });

  document.getElementById("zed-indent").addEventListener("change", (e) => {
    const edit = focusedEdit();
    const block = edit?.closest(".zed-block");
    if (!block || Number(block.dataset.cols || 1) !== 1) {
      toast("Отступ — для блока в одну колонку.", true);
      return;
    }
    setIndent(block, Number(e.target.value));
  });

  document.getElementById("zed-add-p").addEventListener("click", () => {
    const html = renderBlock(normalizeBlock({ cols: 1, html: [""] }), ctx.fields());
    page.insertAdjacentHTML("beforeend", html);
    page.lastElementChild.querySelector(".zed-edit").focus();
  });

  const place = document.getElementById("zed-place-field");
  const refreshPlace = () => {
    if (!place) return;
    const cur = place.value;
    place.innerHTML = `<option value="">Вставить поле…</option>`
      + (ctx.fields() || []).map((f) => `<option value="${esc(f.key)}">${esc(f.label)}</option>`).join("");
    place.value = cur && [...place.options].some((o) => o.value === cur) ? cur : "";
  };
  refreshPlace();
  place?.addEventListener("change", () => {
    const key = place.value;
    place.value = "";
    if (!key) return;
    const field = (ctx.fields() || []).find((f) => f.key === key);
    if (!field) return;
    const edit = focusedEdit();
    if (!edit) {
      toast("Поставьте курсор туда, куда вставить поле", true);
      return;
    }
    insertChipAt(edit, field);
  });

  return {
    harvest: () => harvestLayout(page),
    setBlocks: (blocks) => {
      layout.blocks = blocks;
      page.innerHTML = blocks.map((b) => renderBlock(b, ctx.fields())).join("");
    },
    insertChip: (edit, field) => insertChipAt(edit, field),
    refreshPlace,
  };
}

function insertChipAt(edit, field) {
  edit.focus();
  const chip = document.createElement("span");
  chip.className = "chip";
  chip.dataset.key = field.key;
  chip.contentEditable = "false";
  chip.textContent = field.label;
  const z = document.createTextNode("\u200B");
  const sel = window.getSelection();
  if (sel && sel.rangeCount && edit.contains(sel.anchorNode)) {
    const range = sel.getRangeAt(0);
    range.deleteContents();
    range.insertNode(z);
    range.insertNode(chip);
  } else {
    edit.appendChild(chip);
    edit.appendChild(z);
  }
}

function adjacentChip(edit, dir) {
  const sel = window.getSelection();
  if (!sel || !sel.isCollapsed || sel.rangeCount === 0) return null;
  const node = sel.anchorNode;
  const off = sel.anchorOffset;
  const isChip = (n) => n && n.nodeType === 1 && n.classList && n.classList.contains("chip");
  if (dir === "back") {
    if (node === edit && off > 0) {
      const prev = edit.childNodes[off - 1];
      if (isChip(prev)) return prev;
    }
    if (node.nodeType === 3 && off === 0) {
      let prev = node.previousSibling;
      if (prev && prev.nodeType === 3 && prev.textContent === "\u200B") prev = prev.previousSibling;
      if (isChip(prev)) return prev;
    }
  } else {
    if (node === edit && off < edit.childNodes.length) {
      const next = edit.childNodes[off];
      if (isChip(next)) return next;
    }
    if (node.nodeType === 3 && off === node.textContent.length) {
      let next = node.nextSibling;
      if (next && next.nodeType === 3 && next.textContent === "\u200B") next = next.nextSibling;
      if (isChip(next)) return next;
    }
  }
  return null;
}

function styleChips(page, cmd) {
  const sel = window.getSelection();
  const range = sel && sel.rangeCount ? sel.getRangeAt(0) : null;
  page.querySelectorAll(".chip").forEach((chip) => {
    const hit = chip.classList.contains("chip-on") || (range && range.intersectsNode(chip));
    if (!hit) return;
    if (cmd === "bold") {
      chip.style.fontWeight = chip.style.fontWeight === "bold" || chip.style.fontWeight === "700" ? "" : "bold";
    } else if (cmd === "italic") {
      chip.style.fontStyle = chip.style.fontStyle === "italic" ? "" : "italic";
    } else if (cmd === "underline") {
      chip.style.textDecoration = chip.style.textDecoration.includes("underline") ? "" : "underline";
    }
  });
}
