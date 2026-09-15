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
      if (j && Array.isArray(j.blocks)) return j;
    } catch { /* plain */ }
  }
  const lines = raw ? raw.split(/\n/) : [""];
  return {
    v: 1,
    blocks: lines.map((line) => ({
      id: uid(),
      type: "paragraph",
      align: "left",
      font: "Times New Roman",
      size: 14,
      html: line,
    })),
  };
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
    out += `<span class="chip" data-key="${esc(key)}" contenteditable="false">${esc(f ? f.label : key)}</span>`;
    last = m.index + m[0].length;
  }
  return out + s.slice(last);
}

function dehydrateEl(el) {
  const clone = el.cloneNode(true);
  clone.querySelectorAll(".chip").forEach((chip) => {
    chip.replaceWith(document.createTextNode(`{{${chip.dataset.key}}}`));
  });
  return clone.innerHTML;
}

function harvestLayout(page) {
  const blocks = [];
  page.querySelectorAll(".zed-block").forEach((node) => {
    const type = node.dataset.type;
    const font = node.dataset.font || "Times New Roman";
    const size = Number(node.dataset.size || 14);
    if (type === "header") {
      const left = node.querySelector('[data-side="left"]');
      const right = node.querySelector('[data-side="right"]');
      blocks.push({
        id: node.dataset.id,
        type: "header",
        font,
        size,
        left: dehydrateEl(left),
        right: dehydrateEl(right),
      });
    } else {
      const edit = node.querySelector(".zed-edit");
      blocks.push({
        id: node.dataset.id,
        type: "paragraph",
        align: node.dataset.align || "left",
        font,
        size,
        indent: Number(node.dataset.indent || 0),
        html: dehydrateEl(edit),
      });
    }
  });
  return { v: 1, blocks };
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
  const id = b.id || uid();
  const font = b.font || "Times New Roman";
  const size = b.size || 14;
  const align = b.align || "left";
  const bar = `<div class="zed-block-bar">
    <button type="button" class="ghost compact" data-up title="выше">↑</button>
    <button type="button" class="ghost compact" data-down title="ниже">↓</button>
    <span class="muted">${b.type === "header" ? "шапка" : "абзац"}</span>
    <button type="button" class="ghost compact" data-kind>${b.type === "header" ? "в абзац" : "в шапку"}</button>
    <button type="button" class="ghost compact" data-rm>убрать</button>
  </div>`;
  if (b.type === "header") {
    return `<div class="zed-block" data-id="${esc(id)}" data-type="header" data-font="${esc(font)}" data-size="${size}">
      ${bar}
      <div class="zed-header-row" style="${blockStyle({ font, size, align: "left" })}">
        <div class="zed-edit" data-side="left" contenteditable="true">${hydrateHtml(b.left || "", fields)}</div>
        <div class="zed-edit" data-side="right" contenteditable="true" style="text-align:right">${hydrateHtml(b.right || "", fields)}</div>
      </div>
    </div>`;
  }
  return `<div class="zed-block" data-id="${esc(id)}" data-type="paragraph" data-align="${esc(align)}" data-font="${esc(font)}" data-size="${size}" data-indent="${Number(b.indent || 0)}">
    ${bar}
    <div class="zed-edit" contenteditable="true" style="${blockStyle(b)}">${hydrateHtml(b.html || "", fields)}</div>
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
    <button type="button" class="ghost compact" id="zed-add-p">+ абзац</button>
    <button type="button" class="ghost compact" id="zed-add-h">+ шапка</button>
    <button type="button" class="btn compact" id="zed-field">Поле</button>
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
  const redraw = () => {
    const next = harvestLayout(page);
    layout.blocks = next.blocks;
    page.innerHTML = layout.blocks.map((b) => renderBlock(b, ctx.fields())).join("")
      || renderBlock({ type: "paragraph", html: "" }, ctx.fields());
    bindBlocks();
  };

  const bindBlocks = () => {
    page.querySelectorAll(".zed-edit").forEach((el) => {
      el.addEventListener("focus", () => { window._zedLastEdit = el; });
      el.addEventListener("keydown", (e) => {
        if (e.key !== "Tab") return;
        const block = el.closest(".zed-block");
        if (!block || block.dataset.type !== "paragraph") return;
        e.preventDefault();
        let n = Number(block.dataset.indent || 0);
        n = e.shiftKey ? Math.max(0, n - 1) : Math.min(4, n + 1);
        setIndent(block, n);
      });
    });
    page.querySelectorAll("[data-kind]").forEach((btn) => {
      btn.addEventListener("click", () => {
        const node = btn.closest(".zed-block");
        const one = harvestLayout(node.parentNode).blocks.find((b) => b.id === node.dataset.id);
        if (!one) return;
        const next = one.type === "header"
          ? {
              id: one.id,
              type: "paragraph",
              align: "left",
              font: one.font,
              size: one.size,
              indent: 0,
              html: [one.left, one.right].filter(Boolean).join(" "),
            }
          : {
              id: one.id,
              type: "header",
              font: one.font,
              size: one.size,
              left: one.html || "",
              right: "",
            };
        const wrap = document.createElement("div");
        wrap.innerHTML = renderBlock(next, ctx.fields());
        node.replaceWith(wrap.firstElementChild);
        bindBlocks();
      });
    });
    page.querySelectorAll("[data-up]").forEach((btn) => {
      btn.addEventListener("click", () => {
        const node = btn.closest(".zed-block");
        if (node.previousElementSibling) node.parentNode.insertBefore(node, node.previousElementSibling);
      });
    });
    page.querySelectorAll("[data-down]").forEach((btn) => {
      btn.addEventListener("click", () => {
        const node = btn.closest(".zed-block");
        if (node.nextElementSibling) node.parentNode.insertBefore(node.nextElementSibling, node);
      });
    });
    page.querySelectorAll("[data-rm]").forEach((btn) => {
      btn.addEventListener("click", () => {
        const node = btn.closest(".zed-block");
        node.remove();
        if (!page.querySelector(".zed-block")) {
          layout.blocks = [{ id: uid(), type: "paragraph", align: "left", font: "Times New Roman", size: 14, html: "" }];
          page.innerHTML = renderBlock(layout.blocks[0], ctx.fields());
          bindBlocks();
        }
      });
    });
  };

  document.getElementById("zed-tb").addEventListener("mousedown", (e) => {
    if (e.target.closest("button, select")) e.preventDefault();
  });

  document.querySelectorAll("#zed-tb [data-cmd]").forEach((btn) => {
    btn.addEventListener("click", () => {
      const edit = focusedEdit();
      if (edit) edit.focus();
      document.execCommand("styleWithCSS", false, true);
      document.execCommand(btn.dataset.cmd, false, null);
    });
  });

  document.querySelectorAll("#zed-tb [data-align]").forEach((btn) => {
    btn.addEventListener("click", () => {
      const edit = focusedEdit();
      if (!edit) return;
      const block = edit.closest(".zed-block");
      if (block.dataset.type === "paragraph") {
        block.dataset.align = btn.dataset.align;
        edit.style.textAlign = btn.dataset.align;
      } else {
        edit.style.textAlign = btn.dataset.align;
      }
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
    if (!block || block.dataset.type !== "paragraph") {
      toast("Отступ — для абзаца. Поставьте курсор в строку.", true);
      return;
    }
    setIndent(block, Number(e.target.value));
  });

  document.getElementById("zed-add-p").addEventListener("click", () => {
    const html = renderBlock({
      id: uid(), type: "paragraph", align: "left", font: "Times New Roman", size: 14, html: "",
    }, ctx.fields());
    page.insertAdjacentHTML("beforeend", html);
    bindBlocks();
    page.lastElementChild.querySelector(".zed-edit").focus();
  });

  document.getElementById("zed-add-h").addEventListener("click", () => {
    const html = renderBlock({
      id: uid(), type: "header", font: "Times New Roman", size: 12, left: "", right: "",
    }, ctx.fields());
    page.insertAdjacentHTML("afterbegin", html);
    bindBlocks();
  });

  document.getElementById("zed-field").addEventListener("click", () => {
    const edit = focusedEdit();
    if (!edit) {
      toast("Поставьте курсор в абзац или шапку", true);
      return;
    }
    const sel = window.getSelection();
    const picked = (sel && !sel.isCollapsed && edit.contains(sel.anchorNode))
      ? sel.toString().trim()
      : "";
    ctx.markField(edit, picked);
  });

  bindBlocks();
  return {
    harvest: () => harvestLayout(page),
    setBlocks: (blocks) => {
      layout.blocks = blocks;
      page.innerHTML = blocks.map((b) => renderBlock(b, ctx.fields())).join("");
      bindBlocks();
    },
    insertChip: (edit, field) => {
      edit.focus();
      const chip = document.createElement("span");
      chip.className = "chip";
      chip.dataset.key = field.key;
      chip.contentEditable = "false";
      chip.textContent = field.label;
      const sel = window.getSelection();
      if (sel && sel.rangeCount && edit.contains(sel.anchorNode)) {
        const range = sel.getRangeAt(0);
        range.deleteContents();
        range.insertNode(chip);
      } else {
        edit.appendChild(chip);
      }
    },
    redraw,
  };
}
