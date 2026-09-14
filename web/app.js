const RU = {
  а: "a", б: "b", в: "v", г: "g", д: "d", е: "e", ё: "e", ж: "zh", з: "z",
  и: "i", й: "j", к: "k", л: "l", м: "m", н: "n", о: "o", п: "p", р: "r",
  с: "s", т: "t", у: "u", ф: "f", х: "h", ц: "c", ч: "ch", ш: "sh", щ: "sch",
  ъ: "", ы: "y", ь: "", э: "e", ю: "yu", я: "ya",
};

const FILL = [
  ["manual", "Вписать вручную"],
  ["created_at", "Дата при создании"],
  ["sequence", "Номер по порядку"],
];

function esc(s) {
  return String(s ?? "").replace(/[&<>"'`]/g, (c) => ({
    "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;", "`": "&#96;",
  }[c]));
}

function slugify(s) {
  const mapped = String(s).toLowerCase().split("").map((ch) => RU[ch] ?? ch).join("");
  let slug = mapped.replace(/[^a-z0-9]+/g, "_").replace(/^_|_$/g, "").slice(0, 64);
  if (!slug) slug = "field";
  if (/^[0-9]/.test(slug)) slug = `f_${slug}`;
  return slug;
}

function fmtDate(iso) {
  if (!iso) return "";
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return d.toLocaleString("ru-RU", { dateStyle: "short", timeStyle: "short" });
}

function toast(msg, err = false) {
  const el = document.createElement("div");
  el.className = `toast${err ? " err" : ""}`;
  el.textContent = msg;
  document.getElementById("toasts").appendChild(el);
  setTimeout(() => el.remove(), 4200);
}

async function api(path, opts = {}) {
  const res = await fetch(path, {
    headers: { "Content-Type": "application/json", ...(opts.headers || {}) },
    method: opts.method || "GET",
    body: opts.body !== undefined ? JSON.stringify(opts.body) : undefined,
  });
  if (res.status === 204) return null;
  const data = await res.json().catch(() => ({}));
  if (!res.ok) throw new Error(data.error || res.statusText);
  return data;
}

async function uploadSource(id, file) {
  const fd = new FormData();
  fd.append("file", file);
  const res = await fetch(`/api/documents/${id}/source`, { method: "PUT", body: fd });
  const data = await res.json().catch(() => ({}));
  if (!res.ok) throw new Error(data.error || res.statusText);
  return data;
}

function parseHash() {
  const raw = (location.hash || "#/").replace(/^#/, "");
  const [path, qs] = raw.split("?");
  const parts = path.split("/").filter(Boolean);
  const query = Object.fromEntries(new URLSearchParams(qs || ""));
  return { parts, query };
}

function setNav(active, title, actionsHtml = "") {
  document.getElementById("page-title").textContent = title;
  document.getElementById("page-actions").innerHTML = actionsHtml;
  document.querySelectorAll("[data-nav]").forEach((a) => {
    a.classList.toggle("active", a.dataset.nav === active);
  });
}

async function refreshSidebar(inst) {
  document.getElementById("company-name").textContent = inst.name || "";
  const [templates, docs] = await Promise.all([
    api("/api/templates"),
    api("/api/documents"),
  ]);
  fillNavList("nav-templates", templates.map((t) => ({ href: `#/templates/${t.id}`, title: t.name })));
  fillNavList("nav-documents", docs.map((d) => ({ href: `#/documents/${d.id}`, title: d.title })));
}

function fillNavList(id, items) {
  const box = document.getElementById(id);
  if (!items.length) {
    box.innerHTML = `<span class="muted" style="padding:6px 10px;font-size:13px">пусто</span>`;
    return;
  }
  const shown = items.slice(0, 12);
  box.innerHTML = shown.map((it) => `<a href="${it.href}">${esc(it.title)}</a>`).join("");
}

document.querySelectorAll("[data-toggle]").forEach((btn) => {
  btn.addEventListener("click", (e) => {
    e.preventDefault();
    const box = document.getElementById(`nav-${btn.dataset.toggle}`);
    box.hidden = !box.hidden;
  });
});

function closeModal() {
  const m = document.getElementById("modal");
  m.hidden = true;
  m.innerHTML = "";
}

function openModal(html) {
  const m = document.getElementById("modal");
  m.hidden = false;
  m.innerHTML = `<div class="card modal-card">${html}</div>`;
  m.onclick = (e) => { if (e.target === m) closeModal(); };
}

function hydratePaper(el, text, fields) {
  const s = text || "";
  const re = /\{\{([a-z][a-z0-9_]*)\}\}/g;
  let html = "";
  let last = 0;
  let m;
  while ((m = re.exec(s))) {
    html += esc(s.slice(last, m.index)).replace(/\n/g, "<br>");
    const key = m[1];
    const f = fields.find((x) => x.key === key);
    html += `<span class="chip" data-key="${esc(key)}" contenteditable="false">${esc(f ? f.label : key)}</span>`;
    last = m.index + m[0].length;
  }
  html += esc(s.slice(last)).replace(/\n/g, "<br>");
  el.innerHTML = html;
}

function serializePaper(el) {
  const clone = el.cloneNode(true);
  clone.querySelectorAll("br").forEach((br) => br.replaceWith("\n"));
  clone.querySelectorAll("div,p").forEach((node) => {
    node.insertAdjacentText("beforebegin", "\n");
  });
  clone.querySelectorAll(".chip").forEach((chip) => {
    chip.replaceWith(document.createTextNode(`{{${chip.dataset.key}}}`));
  });
  return (clone.textContent || "").replace(/^\n/, "");
}

function uniqueKey(base, fields) {
  let key = slugify(base);
  let i = 2;
  const taken = new Set(fields.map((f) => f.key));
  const stem = key;
  while (taken.has(key)) key = `${stem}_${i++}`;
  return key;
}

async function render() {
  const view = document.getElementById("view");
  try {
    const inst = await api("/api/instance");
    if (!inst.setup_done) {
      document.body.classList.add("needs-setup");
      showWizard(inst);
      return;
    }
    document.body.classList.remove("needs-setup");
    document.getElementById("wizard").hidden = true;
    await refreshSidebar(inst);
    const { parts, query } = parseHash();
    const a = parts[0] || "home";
    if (a === "settings") await pageSettings(view, inst);
    else if (a === "templates" && parts[1] === "new") await pageTemplateEditor(view, null, query);
    else if (a === "templates" && parts[1]) await pageTemplateEditor(view, parts[1], query);
    else if (a === "templates") await pageTemplates(view);
    else if (a === "documents" && parts[1] === "new") await pageFillDocument(view, query.template);
    else if (a === "documents" && parts[1]) await pageDocumentView(view, parts[1]);
    else if (a === "documents") await pageDocuments(view);
    else if (a === "import" || a === "modules") {
      location.hash = a === "modules" ? "#/settings" : "#/templates/new?import=1";
    } else await pageHome(view);
  } catch (e) {
    view.innerHTML = `<div class="card empty"><h2>Не загрузилось</h2><p>${esc(e.message)}</p></div>`;
    toast(e.message, true);
  }
}

function showWizard(inst) {
  const box = document.getElementById("wizard");
  box.hidden = false;
  const selected = new Set((inst.modules || []).filter((m) => m.enabled).map((m) => m.id));
  const modules = inst.modules || [];
  box.innerHTML = `
    <div class="card wizard-card">
      <p class="kicker">ZeroEffortDocs</p>
      <h1>Настройка компании</h1>
      <p class="muted">Название и модули сохраняются сразу. Модули «скоро» тоже можно включить — когда появятся, уже будут на месте.</p>
      <form id="wiz" class="form" style="margin-top:18px">
        <label><span>Название компании</span>
          <input name="name" type="text" required value="${esc(inst.name || "")}" placeholder="ООО «Ромашка»">
        </label>
        <div>
          <div class="kicker">Модули</div>
          <div class="module-pick" id="mod-pick">
            ${modules.map((m) => `
              <div class="card ${selected.has(m.id) ? "on" : ""}" data-id="${esc(m.id)}">
                <div class="spread">
                  <div>
                    <h3>${esc(m.name)} ${m.available ? "" : `<span class="badge warn">скоро</span>`}</h3>
                    <p class="muted">${esc(m.description)}</p>
                  </div>
                  <button type="button" class="switch ${selected.has(m.id) ? "on" : ""}" data-id="${esc(m.id)}"></button>
                </div>
              </div>`).join("")}
          </div>
        </div>
        <button type="submit">Продолжить</button>
      </form>
    </div>`;
  const toggle = (id) => {
    if (selected.has(id)) selected.delete(id);
    else selected.add(id);
    box.querySelectorAll("[data-id]").forEach((el) => {
      const on = selected.has(el.dataset.id);
      el.classList.toggle("on", on);
      if (el.classList.contains("switch")) el.classList.toggle("on", on);
    });
  };
  box.querySelectorAll(".module-pick .card").forEach((card) => {
    card.addEventListener("click", () => toggle(card.dataset.id));
  });
  document.getElementById("wiz").addEventListener("submit", async (ev) => {
    ev.preventDefault();
    try {
      await api("/api/setup", {
        method: "POST",
        body: { name: ev.target.name.value, enabled_modules: [...selected] },
      });
      toast("Компания сохранена");
      location.hash = "#/";
      render();
    } catch (e) {
      toast(e.message, true);
    }
  });
}

async function pageHome(view) {
  setNav("home", "Обзор", `<button type="button" class="btn" id="btn-create">Создать</button>`);
  const [stats, docs] = await Promise.all([api("/api/stats"), api("/api/documents")]);
  const recent = docs.slice(0, 8);
  view.innerHTML = `
    <div class="stats">
      <div class="card"><div class="kicker">Шаблоны</div><div class="stat-num">${stats.templates}</div></div>
      <div class="card"><div class="kicker">Документы</div><div class="stat-num">${stats.documents}</div></div>
    </div>
    <div class="card" style="margin-top:20px">
      <div class="spread"><h2>История документов</h2><a href="#/documents">все</a></div>
      ${recent.length === 0 ? `<p class="muted" style="margin-top:8px">Пока пусто — создайте первый документ.</p>` : `
        <table class="table">
          <tbody>
            ${recent.map((d) => `<tr><td><a href="#/documents/${d.id}">${esc(d.title)}</a></td><td class="muted">${esc(d.template_name)}</td><td class="muted">${esc(fmtDate(d.updated_at))}</td></tr>`).join("")}
          </tbody>
        </table>`}
    </div>`;
  document.getElementById("btn-create").addEventListener("click", () => openCreateChooser(stats.templates > 0));
}

function openCreateChooser(hasTemplates) {
  openModal(`
    <h2>Создать</h2>
    <p class="muted">Новый документ — чистый лист, из него можно сделать шаблон. По шаблону — заполнить готовую форму.</p>
    <div class="grid" style="margin-top:16px">
      <button type="button" class="btn" id="c-new">Новый документ</button>
      <button type="button" class="btn ghost" id="c-tmpl" ${hasTemplates ? "" : "disabled"}>По шаблону</button>
    </div>`);
  document.getElementById("c-new").addEventListener("click", () => {
    closeModal();
    location.hash = "#/templates/new";
  });
  document.getElementById("c-tmpl")?.addEventListener("click", () => {
    if (!hasTemplates) return;
    closeModal();
    location.hash = "#/documents/new";
  });
}

async function pageSettings(view, inst) {
  setNav("settings", "Настройки", "");
  const selected = new Set((inst.modules || []).filter((m) => m.enabled).map((m) => m.id));
  view.innerHTML = `
    <form class="form" id="set-form">
      <label><span>Название компании</span>
        <input name="name" type="text" required value="${esc(inst.name)}">
      </label>
      <div>
        <div class="kicker">Модули</div>
        <div class="module-pick" id="mod-pick">
          ${(inst.modules || []).map((m) => `
            <div class="card ${selected.has(m.id) ? "on" : ""}" data-id="${esc(m.id)}">
              <div class="spread">
                <div>
                  <h3>${esc(m.name)} ${m.available ? "" : `<span class="badge warn">скоро</span>`}</h3>
                  <p class="muted">${esc(m.description)}</p>
                </div>
                <button type="button" class="switch ${selected.has(m.id) ? "on" : ""}" data-id="${esc(m.id)}"></button>
              </div>
            </div>`).join("")}
        </div>
      </div>
      <button type="submit">Сохранить</button>
    </form>`;
  const paint = () => {
    view.querySelectorAll("[data-id]").forEach((el) => {
      const on = selected.has(el.dataset.id);
      el.classList.toggle("on", on);
    });
  };
  view.querySelectorAll(".module-pick .card").forEach((card) => {
    card.addEventListener("click", () => {
      if (selected.has(card.dataset.id)) selected.delete(card.dataset.id);
      else selected.add(card.dataset.id);
      paint();
    });
  });
  document.getElementById("set-form").addEventListener("submit", async (ev) => {
    ev.preventDefault();
    try {
      const saved = await api("/api/instance", {
        method: "PATCH",
        body: { name: ev.target.name.value, enabled_modules: [...selected] },
      });
      document.getElementById("company-name").textContent = saved.name;
      toast("Настройки сохранены");
    } catch (e) {
      toast(e.message, true);
    }
  });
}

async function pageTemplates(view) {
  setNav(
    "templates",
    "Шаблоны",
    `<a class="btn ghost" href="#/templates/new?import=1">Импортировать</a><a class="btn" href="#/templates/new">Создать</a>`,
  );
  document.getElementById("nav-templates").hidden = false;
  const items = await api("/api/templates");
  if (!items.length) {
    view.innerHTML = `<div class="card empty"><h2>Шаблонов нет</h2><p>Создайте лист или импортируйте существующий приказ — отметьте ФИО, дату, номер, и справа появится форма.</p></div>`;
    return;
  }
  view.innerHTML = `<div class="list">${items.map((t) => `
    <a class="card clickable" href="#/templates/${t.id}" style="text-decoration:none;color:inherit">
      <div class="spread">
        <div>
          <h3>${esc(t.name)}</h3>
          <p class="muted">${t.field_count} полей · ${t.document_count} док.</p>
        </div>
      </div>
    </a>`).join("")}</div>`;
}

async function pageTemplateEditor(view, id, query) {
  const isNew = !id;
  setNav("templates", isNew ? "Новый документ" : "Шаблон", "");
  const data = isNew
    ? { name: "", body: "", fields: [], document_count: 0 }
    : await api(`/api/templates/${id}`);
  const st = {
    name: data.name,
    fields: (data.fields || []).map((f) => ({
      id: f.id,
      key: f.key,
      label: f.label,
      type: f.type,
      required: f.required,
      fill_mode: f.fill_mode || "manual",
      options: f.options || [],
    })),
    file: null,
  };

  const draw = () => {
    view.innerHTML = `
      <div class="sheet-grid">
        <div class="grid">
          <label><span>Название шаблона</span>
            <input id="tmpl-name" type="text" required value="${esc(st.name)}" placeholder="Приём на работу">
          </label>
          ${isNew && query.import ? `
            <label class="drop" id="drop">
              <input type="file" id="file">
              <strong>Исходный документ</strong>
              <p class="muted" id="file-label">${st.file ? esc(st.file.name) : "Перетащите файл или нажмите. Текст из .txt попадёт на лист."}</p>
            </label>` : ""}
          <div>
            <div class="spread" style="margin-bottom:8px">
              <span class="muted">Лист — как в Word. Выделите ФИО, дату или номер и нажмите «Поле».</span>
              <button type="button" class="ghost compact" id="mark-field">Поле</button>
            </div>
            <div class="paper-wrap" id="paper" contenteditable="true"></div>
          </div>
        </div>
        <div class="grid">
          <h2>Форма</h2>
          <p class="muted">Справа то, что будут заполнять вместо поиска по договору.</p>
          <div class="list" id="field-list"></div>
          <div class="row">
            <button type="button" class="btn" id="save-tmpl">Сохранить шаблон</button>
            ${isNew ? "" : `<button type="button" class="ghost" id="issue">Документ по шаблону</button>`}
            ${isNew || data.document_count ? "" : `<button type="button" class="danger ghost" id="del-tmpl">удалить</button>`}
          </div>
        </div>
      </div>`;
    hydratePaper(document.getElementById("paper"), data.body, st.fields);
    paintFields();
    bindEditor();
  };

  const paintFields = () => {
    const box = document.getElementById("field-list");
    if (!st.fields.length) {
      box.innerHTML = `<p class="muted">Полей пока нет. Выделите место на листе.</p>`;
      return;
    }
    box.innerHTML = st.fields.map((f, i) => `
      <div class="field-card" data-i="${i}">
        <label><span>Подпись</span><input data-k="label" type="text" value="${esc(f.label)}"></label>
        <label><span>Как заполнять</span>
          <select data-k="fill_mode">${FILL.map(([v, l]) => `<option value="${v}" ${f.fill_mode === v ? "selected" : ""}>${l}</option>`).join("")}</select>
        </label>
        ${f.fill_mode === "manual" ? `<label class="check"><input data-k="required" type="checkbox" ${f.required ? "checked" : ""}> обязательно</label>` : `<p class="muted">${f.fill_mode === "created_at" ? "Подставится дата создания документа." : "Номер хранится и растёт сам."}</p>`}
        <button type="button" class="ghost compact" data-rm>убрать</button>
      </div>`).join("");
    box.querySelectorAll("[data-k]").forEach((el) => {
      el.addEventListener("change", () => readFields());
      el.addEventListener("input", () => {
        if (el.dataset.k === "label") {
          const i = Number(el.closest(".field-card").dataset.i);
          const chip = document.querySelector(`.chip[data-key="${st.fields[i].key}"]`);
          if (chip) chip.textContent = el.value;
        }
      });
    });
    box.querySelectorAll("[data-rm]").forEach((btn) => {
      btn.addEventListener("click", () => {
        readFields();
        const i = Number(btn.closest(".field-card").dataset.i);
        const key = st.fields[i].key;
        st.fields.splice(i, 1);
        document.querySelectorAll(`.chip[data-key="${key}"]`).forEach((chip) => {
          chip.replaceWith(document.createTextNode(chip.textContent));
        });
        paintFields();
      });
    });
  };

  const readFields = () => {
    view.querySelectorAll(".field-card").forEach((card) => {
      const f = st.fields[Number(card.dataset.i)];
      f.label = card.querySelector('[data-k="label"]').value;
      f.fill_mode = card.querySelector('[data-k="fill_mode"]').value;
      const req = card.querySelector('[data-k="required"]');
      f.required = f.fill_mode === "manual" ? !!req?.checked : false;
      if (f.fill_mode === "created_at") f.type = "date";
      else if (f.fill_mode === "sequence") f.type = "number";
      else f.type = "text";
    });
  };

  const bindEditor = () => {
    document.getElementById("mark-field").addEventListener("click", () => {
      const sel = window.getSelection();
      if (!sel || !sel.rangeCount || sel.isCollapsed) {
        toast("Выделите фрагмент на листе", true);
        return;
      }
      const paper = document.getElementById("paper");
      if (!paper.contains(sel.anchorNode)) {
        toast("Выделение должно быть на листе", true);
        return;
      }
      const text = sel.toString().trim();
      if (!text) return;
      const key = uniqueKey(text, st.fields);
      st.fields.push({
        key, label: text, type: "text", required: true, fill_mode: "manual", options: [],
      });
      const chip = document.createElement("span");
      chip.className = "chip";
      chip.dataset.key = key;
      chip.contentEditable = "false";
      chip.textContent = text;
      const range = sel.getRangeAt(0);
      range.deleteContents();
      range.insertNode(chip);
      sel.removeAllRanges();
      paintFields();
    });
    document.getElementById("save-tmpl").addEventListener("click", async () => {
      readFields();
      st.name = document.getElementById("tmpl-name").value;
      const body = serializePaper(document.getElementById("paper"));
      try {
        const payload = {
          name: st.name,
          description: "",
          body,
          fields: st.fields.map((f) => ({
            id: f.id,
            key: f.key,
            label: f.label,
            type: f.type,
            required: f.required,
            fill_mode: f.fill_mode,
            options: f.options || [],
          })),
        };
        const saved = isNew
          ? await api("/api/templates", { method: "POST", body: payload })
          : await api(`/api/templates/${id}`, { method: "PUT", body: payload });
        if (st.file) {
          const first = await api("/api/documents", {
            method: "POST",
            body: { template_id: saved.id, title: `${saved.name} — оригинал`, body: saved.body, values: {} },
          });
          try {
            await uploadSource(first.id, st.file);
          } catch (e) {
            toast(`Шаблон есть, файл нет: ${e.message}`, true);
          }
        }
        toast("Шаблон сохранён");
        location.hash = `#/templates/${saved.id}`;
        if (!isNew) render();
      } catch (e) {
        toast(e.message, true);
      }
    });
    document.getElementById("issue")?.addEventListener("click", () => {
      location.hash = `#/documents/new?template=${id}`;
    });
    document.getElementById("del-tmpl")?.addEventListener("click", async () => {
      if (!confirm("Удалить шаблон?")) return;
      try {
        await api(`/api/templates/${id}`, { method: "DELETE" });
        toast("Удалено");
        location.hash = "#/templates";
      } catch (e) {
        toast(e.message, true);
      }
    });
    const drop = document.getElementById("drop");
    const fileInput = document.getElementById("file");
    if (fileInput) {
      const onFile = async (file) => {
        st.file = file;
        document.getElementById("file-label").textContent = file.name;
        if (/\.txt$/i.test(file.name) || file.type.startsWith("text/")) {
          const text = await file.text();
          data.body = text;
          hydratePaper(document.getElementById("paper"), text, st.fields);
        }
        if (!document.getElementById("tmpl-name").value) {
          document.getElementById("tmpl-name").value = file.name.replace(/\.[^.]+$/, "");
        }
      };
      fileInput.addEventListener("change", () => { if (fileInput.files[0]) onFile(fileInput.files[0]); });
      drop.addEventListener("dragover", (e) => { e.preventDefault(); drop.classList.add("drag"); });
      drop.addEventListener("dragleave", () => drop.classList.remove("drag"));
      drop.addEventListener("drop", (e) => {
        e.preventDefault();
        drop.classList.remove("drag");
        if (e.dataTransfer.files[0]) onFile(e.dataTransfer.files[0]);
      });
    }
  };

  draw();
}

async function pageDocuments(view) {
  setNav("documents", "Документы", `<button type="button" class="btn" id="btn-create">Создать</button>`);
  document.getElementById("nav-documents").hidden = false;
  const [docs, templates] = await Promise.all([api("/api/documents"), api("/api/templates")]);
  document.getElementById("btn-create").addEventListener("click", () => openCreateChooser(templates.length > 0));
  if (!docs.length) {
    view.innerHTML = `<div class="card empty"><h2>Документов нет</h2><p>Создайте новый лист или заполните шаблон.</p></div>`;
    return;
  }
  view.innerHTML = `<div class="card" style="padding:8px 16px">
    <table class="table">
      <thead><tr><th>Документ</th><th>Шаблон</th><th>Изменён</th></tr></thead>
      <tbody>
        ${docs.map((d) => `<tr><td><a href="#/documents/${d.id}">${esc(d.title)}</a>${d.has_source ? ` <span class="badge">файл</span>` : ""}</td><td>${esc(d.template_name)}</td><td class="muted">${esc(fmtDate(d.updated_at))}</td></tr>`).join("")}
      </tbody>
    </table>
  </div>`;
}

async function pageFillDocument(view, templateId) {
  const templates = await api("/api/templates");
  if (!templates.length) {
    location.hash = "#/templates/new";
    return;
  }
  const tid = templateId || templates[0].id;
  const t = await api(`/api/templates/${tid}`);
  setNav("documents", "По шаблону", "");
  const manual = t.fields.filter((f) => (f.fill_mode || "manual") === "manual");
  view.innerHTML = `
    <form class="form form-wide" id="fill">
      <label><span>Шаблон</span>
        <select id="tmpl-pick">${templates.map((x) => `<option value="${esc(x.id)}" ${x.id === t.id ? "selected" : ""}>${esc(x.name)}</option>`).join("")}</select>
      </label>
      <label><span>Название записи</span><input name="title" type="text" required placeholder="${esc(t.name)}"></label>
      ${manual.map((f) => `<label><span>${esc(f.label)}${f.required ? " *" : ""}</span>${fieldControl(f)}</label>`).join("")}
      ${t.fields.filter((f) => f.fill_mode && f.fill_mode !== "manual").map((f) => `<p class="muted">${esc(f.label)}: ${f.fill_mode === "created_at" ? "дата подставится сама" : "номер выдаст система"}</p>`).join("")}
      <div class="row"><button type="submit">Создать документ</button></div>
    </form>`;
  document.getElementById("tmpl-pick").addEventListener("change", (e) => {
    location.hash = `#/documents/new?template=${e.target.value}`;
  });
  document.getElementById("fill").addEventListener("submit", async (ev) => {
    ev.preventDefault();
    const values = {};
    for (const f of manual) {
      const raw = ev.target[f.key]?.value ?? "";
      values[f.key] = f.type === "number" && raw !== "" ? Number(raw) : raw;
    }
    try {
      const saved = await api("/api/documents", {
        method: "POST",
        body: { template_id: t.id, title: ev.target.title.value, body: "", values },
      });
      toast("Документ создан");
      location.hash = `#/documents/${saved.id}`;
    } catch (e) {
      toast(e.message, true);
    }
  });
}

function fieldControl(field) {
  if (field.type === "textarea") return `<textarea name="${esc(field.key)}"></textarea>`;
  if (field.type === "select") {
    const opts = [`<option value="">—</option>`].concat((field.options || []).map((o) => `<option>${esc(o)}</option>`));
    return `<select name="${esc(field.key)}">${opts.join("")}</select>`;
  }
  if (field.type === "checkbox") {
    return `<label class="check"><input name="${esc(field.key)}" type="checkbox"> да</label>`;
  }
  const t = field.type === "number" ? "number" : field.type === "date" ? "date" : "text";
  return `<input name="${esc(field.key)}" type="${t}">`;
}

async function pageDocumentView(view, id) {
  const doc = await api(`/api/documents/${id}`);
  setNav("documents", doc.title, "");
  const source = doc.source
    ? `<p><a class="btn ghost compact" href="/api/documents/${doc.id}/source">скачать ${esc(doc.source.name)}</a></p>`
    : "";
  view.innerHTML = `
    <div class="sheet-grid">
      <div>
        ${source}
        <div class="paper-wrap" id="paper"></div>
      </div>
      <div class="card">
        <p class="muted">Шаблон: <a href="#/templates/${doc.template.id}">${esc(doc.template.name)}</a></p>
        <dl>
          ${doc.template.fields.map((f) => `<p><span class="muted">${esc(f.label)}</span><br>${esc(fmtValue(doc.values[f.key]))}</p>`).join("")}
        </dl>
        <button type="button" class="danger ghost" id="del-doc">удалить</button>
      </div>
    </div>`;
  hydratePaper(document.getElementById("paper"), doc.body, doc.template.fields);
  document.getElementById("paper").contentEditable = "false";
  document.getElementById("del-doc").addEventListener("click", async () => {
    if (!confirm("Удалить документ?")) return;
    try {
      await api(`/api/documents/${id}`, { method: "DELETE" });
      toast("Удалено");
      location.hash = "#/documents";
    } catch (e) {
      toast(e.message, true);
    }
  });
}

function fmtValue(v) {
  if (v == null) return "—";
  if (v === true) return "да";
  if (v === false) return "нет";
  return String(v);
}

window.addEventListener("hashchange", render);
render();
