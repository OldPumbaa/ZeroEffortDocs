const RU = {
  а: "a", б: "b", в: "v", г: "g", д: "d", е: "e", ё: "e", ж: "zh", з: "z",
  и: "i", й: "j", к: "k", л: "l", м: "m", н: "n", о: "o", п: "p", р: "r",
  с: "s", т: "t", у: "u", ф: "f", х: "h", ц: "c", ч: "ch", ш: "sh", щ: "sch",
  ъ: "", ы: "y", ь: "", э: "e", ю: "yu", я: "ya",
};

const TYPES = [
  ["text", "Строка"],
  ["textarea", "Текст"],
  ["number", "Число"],
  ["date", "Дата"],
  ["checkbox", "Да / нет"],
  ["select", "Список"],
];

const SAMPLE_HIRE = {
  name: "Приём на работу",
  description: "Регистрация нового сотрудника без копирования прошлого приказа.",
  fields: [
    { key: "full_name", label: "ФИО", type: "text", required: true, options: [] },
    { key: "position", label: "Должность", type: "text", required: true, options: [] },
    { key: "department", label: "Подразделение", type: "text", required: false, options: [] },
    { key: "start_date", label: "Дата приёма", type: "date", required: true, options: [] },
    { key: "salary", label: "Оклад", type: "number", required: false, options: [] },
    { key: "contract_type", label: "Тип договора", type: "select", required: true, options: ["Трудовой", "Срочный", "ГПХ"] },
    { key: "probation", label: "Испытательный срок", type: "checkbox", required: false, options: [] },
    { key: "comment", label: "Комментарий", type: "textarea", required: false, options: [] },
  ],
};

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

async function loadCompany() {
  const inst = await api("/api/instance");
  document.getElementById("company-name").textContent = inst.name;
  return inst;
}

document.getElementById("rename-company").addEventListener("click", async () => {
  const current = document.getElementById("company-name").textContent;
  const name = prompt("Название компании", current);
  if (!name) return;
  try {
    const inst = await api("/api/instance", { method: "PATCH", body: { name } });
    document.getElementById("company-name").textContent = inst.name;
  } catch (e) {
    toast(e.message, true);
  }
});

async function render() {
  const view = document.getElementById("view");
  try {
    await loadCompany();
    const { parts, query } = parseHash();
    const a = parts[0] || "home";
    if (a === "import") await pageImport(view, query);
    else if (a === "templates" && parts[1] === "new") await pageTemplateEditor(view, null);
    else if (a === "templates" && parts[1]) await pageTemplateEditor(view, parts[1]);
    else if (a === "templates") await pageTemplates(view);
    else if (a === "documents" && parts[1] === "new") await pageDocumentEditor(view, null, query.template);
    else if (a === "documents" && parts[1]) await pageDocumentEditor(view, parts[1]);
    else if (a === "documents") await pageDocuments(view, query.template);
    else if (a === "modules") await pageModules(view);
    else await pageHome(view);
  } catch (e) {
    view.innerHTML = `<div class="card empty"><h2>Не загрузилось</h2><p>${esc(e.message)}</p></div>`;
    toast(e.message, true);
  }
}

async function pageHome(view) {
  setNav("home", "Обзор", `<a class="btn" href="#/import">Импорт</a>`);
  const [stats, docs] = await Promise.all([
    api("/api/stats"),
    api("/api/documents"),
  ]);
  const recent = docs.slice(0, 6);
  view.innerHTML = `
    <div class="stats">
      <div class="card"><div class="kicker">Формы</div><div class="stat-num">${stats.templates}</div></div>
      <div class="card"><div class="kicker">Документы</div><div class="stat-num">${stats.documents}</div></div>
      <div class="card"><div class="kicker">Модули</div><div class="stat-num">${stats.modules_enabled}</div><p class="muted">включено заранее</p></div>
    </div>
    <div class="grid" style="margin-top:20px">
      <div class="card">
        <h2>С чего начать</h2>
        <p class="muted">Добавьте документ: простой редактор, свои поля — из них соберётся форма. Следующий такой же уже заполняется по форме, без копирования прошлого файла.</p>
        <p style="margin-top:12px" class="row">
          <a class="btn" href="#/import">Импортировать документ</a>
        </p>
      </div>
      <div class="card">
        <div class="spread"><h2>Последние документы</h2><a href="#/documents">все</a></div>
        ${recent.length === 0 ? `<p class="muted" style="margin-top:8px">Пока пусто.</p>` : `
          <table class="table">
            <tbody>
              ${recent.map((d) => `<tr><td><a href="#/documents/${d.id}">${esc(d.title)}</a></td><td class="muted">${esc(d.template_name)}</td><td class="muted">${esc(fmtDate(d.updated_at))}</td></tr>`).join("")}
            </tbody>
          </table>`}
      </div>
    </div>`;
}

async function seedHire() {
  try {
    const t = await api("/api/templates", { method: "POST", body: SAMPLE_HIRE });
    toast("Форма «Приём на работу» создана");
    location.hash = `#/templates/${t.id}`;
  } catch (e) {
    toast(e.message, true);
  }
}

async function pageTemplates(view) {
  setNav(
    "templates",
    "Формы",
    `<button type="button" class="ghost" id="seed-hire">пример</button><a class="btn" href="#/import">Импорт</a><a class="btn ghost" href="#/templates/new">Новая форма</a>`,
  );
  const items = await api("/api/templates");
  if (items.length === 0) {
    view.innerHTML = `<div class="card empty"><h2>Форм ещё нет</h2><p>Форма появляется из полей при импорте. Можно собрать и отдельно, если вид документа уже известен.</p></div>`;
  } else {
    view.innerHTML = `<div class="list">${items.map((t) => `
      <a class="card clickable" href="#/templates/${t.id}" style="text-decoration:none;color:inherit">
        <div class="spread">
          <div>
            <h3>${esc(t.name)}</h3>
            <p class="muted">${esc(t.description) || "без описания"}</p>
          </div>
          <div class="muted">${t.field_count} полей · ${t.document_count} док.</div>
        </div>
      </a>`).join("")}</div>`;
  }
  document.getElementById("seed-hire")?.addEventListener("click", seedHire);
}

function blankField() {
  return { id: null, key: "", label: "", type: "text", required: false, options: [], value: "" };
}

async function uploadSource(id, file) {
  const fd = new FormData();
  fd.append("file", file);
  const res = await fetch(`/api/documents/${id}/source`, { method: "PUT", body: fd });
  const data = await res.json().catch(() => ({}));
  if (!res.ok) throw new Error(data.error || res.statusText);
  return data;
}

async function pageImport(view, query) {
  setNav("import", "Импорт", "");
  const templates = await api("/api/templates");
  const st = {
    title: "",
    body: "",
    file: null,
    formMode: query.template ? "existing" : "new",
    formId: query.template || "",
    formName: "",
    fields: SAMPLE_HIRE.fields.map((f) => ({ ...f, id: null, value: f.type === "checkbox" ? false : "" })),
    existing: null,
    values: {},
  };
  if (st.formMode === "existing" && !st.formId && templates[0]) st.formId = templates[0].id;
  if (st.formMode === "new") st.fields = [blankField()];

  const loadExisting = async () => {
    if (st.formMode === "existing" && st.formId) {
      st.existing = await api(`/api/templates/${st.formId}`);
    } else {
      st.existing = null;
    }
  };
  await loadExisting();

  const readUi = () => {
    const form = document.getElementById("import-form");
    if (!form) return;
    st.title = form.title.value;
    st.body = form.body.value;
    st.formName = form.form_name ? form.form_name.value : st.formName;
    if (st.formMode === "new") {
      view.querySelectorAll(".field-card").forEach((card) => {
        const i = Number(card.dataset.i);
        const f = st.fields[i];
        f.label = card.querySelector('[data-k="label"]').value;
        const keyEl = card.querySelector('[data-k="key"]');
        f.key = keyEl.value;
        f.type = card.querySelector('[data-k="type"]').value;
        f.required = card.querySelector('[data-k="required"]').checked;
        const opt = card.querySelector('[data-k="options"]');
        f.options = opt ? opt.value.split(",").map((s) => s.trim()).filter(Boolean) : [];
        const val = card.querySelector('[data-k="value"]');
        if (f.type === "checkbox") f.value = !!val?.checked;
        else if (f.type === "number") f.value = val?.value === "" ? null : Number(val.value);
        else f.value = val?.value ?? "";
      });
    } else if (st.existing) {
      st.values = collectValues(form, st.existing.fields);
    }
  };

  const draw = () => {
    const fileLabel = st.file ? st.file.name : "Перетащите файл или нажмите — PDF, Word, скан, что угодно из канцелярии";
    view.innerHTML = `
      <form class="form-wide" id="import-form">
        <div class="editor-grid">
          <div class="grid">
            <label class="drop" id="drop">
              <input type="file" id="file">
              <strong>Исходный файл</strong>
              <p class="muted" id="file-label">${esc(fileLabel)}</p>
            </label>
            <label><span>Название записи</span><input name="title" type="text" required value="${esc(st.title)}" placeholder="Иванов И. И. — приём"></label>
            <label><span>Текст документа</span><textarea name="body" class="editor-body" placeholder="Пока простой редактор: вставьте или наберите текст. Полноценный Word — позже.">${esc(st.body)}</textarea></label>
          </div>
          <div class="grid">
            <label><span>Форма</span>
              <select id="form-mode">
                <option value="new" ${st.formMode === "new" ? "selected" : ""}>новая — указать поля</option>
                ${templates.map((t) => `<option value="${esc(t.id)}" ${st.formMode === "existing" && st.formId === t.id ? "selected" : ""}>уже есть: ${esc(t.name)}</option>`).join("")}
              </select>
            </label>
            ${st.formMode === "new" ? `
              <label><span>Как назовём форму</span><input name="form_name" type="text" required value="${esc(st.formName)}" placeholder="Приём на работу"></label>
              <div>
                <div class="spread"><h2>Поля</h2><div class="row"><button type="button" class="ghost compact" id="seed-fields">пример</button><button type="button" class="ghost compact" id="add-field">добавить поле</button></div></div>
                <p class="muted" style="margin:0 0 8px">Подпись и тип — из этого соберётся форма для следующих документов.</p>
                <div class="list" id="fields"></div>
              </div>
            ` : st.existing ? `
              <p class="muted">Форма «${esc(st.existing.name)}». Заполните поля этой записи.</p>
              ${st.existing.fields.map((f) => `<label><span>${esc(f.label)}${f.required ? " *" : ""}</span>${fieldControl(f, st.values[f.key])}</label>`).join("")}
            ` : `<p class="muted">Форм ещё нет — оставьте «новая».</p>`}
            <div class="row"><button type="submit">Сохранить</button></div>
          </div>
        </div>
      </form>`;
    if (st.formMode === "new") paintImportFields();
    bindImport();
  };

  const paintImportFields = () => {
    const box = document.getElementById("fields");
    box.innerHTML = st.fields.map((f, i) => `
      <div class="field-card" data-i="${i}">
        <div class="field-grid">
          <label><span>Подпись</span><input data-k="label" type="text" value="${esc(f.label)}" placeholder="ФИО"></label>
          <label><span>Ключ</span><input data-k="key" type="text" value="${esc(f.key)}" placeholder="full_name"></label>
          <label><span>Тип</span>
            <select data-k="type">${TYPES.map(([v, l]) => `<option value="${v}" ${f.type === v ? "selected" : ""}>${l}</option>`).join("")}</select>
          </label>
          <label class="check"><input data-k="required" type="checkbox" ${f.required ? "checked" : ""} ${f.type === "checkbox" ? "disabled" : ""}> обязательно</label>
        </div>
        ${f.type === "select" ? `<label><span>Варианты через запятую</span><input data-k="options" type="text" value="${esc((f.options || []).join(", "))}" placeholder="Трудовой, ГПХ"></label>` : ""}
        <label><span>Значение в этом документе</span>${valueControl(f)}</label>
        <div class="row">
          <button type="button" class="ghost compact" data-up ${i === 0 ? "disabled" : ""}>↑</button>
          <button type="button" class="ghost compact" data-down ${i === st.fields.length - 1 ? "disabled" : ""}>↓</button>
          <button type="button" class="ghost compact" data-rm>убрать</button>
        </div>
      </div>`).join("");
  };

  const bindImport = () => {
    const drop = document.getElementById("drop");
    const fileInput = document.getElementById("file");
    const onFile = (file) => {
      st.file = file;
      document.getElementById("file-label").textContent = file
        ? file.name
        : "Перетащите файл или нажмите — PDF, Word, скан, что угодно из канцелярии";
    };
    fileInput.addEventListener("change", () => onFile(fileInput.files[0] || null));
    drop.addEventListener("dragover", (e) => { e.preventDefault(); drop.classList.add("drag"); });
    drop.addEventListener("dragleave", () => drop.classList.remove("drag"));
    drop.addEventListener("drop", (e) => {
      e.preventDefault();
      drop.classList.remove("drag");
      if (e.dataTransfer.files[0]) onFile(e.dataTransfer.files[0]);
    });
    document.getElementById("form-mode").addEventListener("change", async (e) => {
      readUi();
      const v = e.target.value;
      if (v === "new") {
        st.formMode = "new";
        st.formId = "";
      } else {
        st.formMode = "existing";
        st.formId = v;
        await loadExisting();
      }
      draw();
    });
    document.getElementById("add-field")?.addEventListener("click", () => {
      readUi();
      st.fields.push(blankField());
      draw();
    });
    document.getElementById("seed-fields")?.addEventListener("click", () => {
      readUi();
      st.formName = st.formName || SAMPLE_HIRE.name;
      st.fields = SAMPLE_HIRE.fields.map((f) => ({ ...f, id: null, value: f.type === "checkbox" ? false : "" }));
      draw();
    });
    view.querySelectorAll('[data-k="label"]').forEach((input) => {
      input.addEventListener("input", () => {
        const key = input.closest(".field-card").querySelector('[data-k="key"]');
        if (!key.dataset.touched) key.value = slugify(input.value);
      });
    });
    view.querySelectorAll('[data-k="key"]').forEach((input) => {
      input.addEventListener("input", () => { input.dataset.touched = "1"; });
    });
    view.querySelectorAll('[data-k="type"]').forEach((sel) => {
      sel.addEventListener("change", () => { readUi(); draw(); });
    });
    view.querySelectorAll("[data-up]").forEach((btn) => btn.addEventListener("click", () => {
      const i = Number(btn.closest(".field-card").dataset.i);
      readUi();
      [st.fields[i - 1], st.fields[i]] = [st.fields[i], st.fields[i - 1]];
      draw();
    }));
    view.querySelectorAll("[data-down]").forEach((btn) => btn.addEventListener("click", () => {
      const i = Number(btn.closest(".field-card").dataset.i);
      readUi();
      [st.fields[i + 1], st.fields[i]] = [st.fields[i], st.fields[i + 1]];
      draw();
    }));
    view.querySelectorAll("[data-rm]").forEach((btn) => btn.addEventListener("click", () => {
      const i = Number(btn.closest(".field-card").dataset.i);
      readUi();
      st.fields.splice(i, 1);
      if (!st.fields.length) st.fields.push(blankField());
      draw();
    }));
    document.getElementById("import-form").addEventListener("submit", async (ev) => {
      ev.preventDefault();
      readUi();
      const values = {};
      let fields = [];
      if (st.formMode === "new") {
        fields = st.fields.map((f) => ({
          key: f.key || slugify(f.label),
          label: f.label,
          type: f.type,
          required: f.required,
          options: f.options || [],
        }));
        st.fields.forEach((f) => { values[f.key || slugify(f.label)] = f.value; });
      } else {
        Object.assign(values, st.values);
      }
      try {
        const saved = await api("/api/import", {
          method: "POST",
          body: {
            title: st.title,
            body: st.body,
            form_id: st.formMode === "existing" ? st.formId : null,
            form_name: st.formMode === "new" ? st.formName : null,
            fields,
            values,
          },
        });
        if (st.file) {
          try {
            await uploadSource(saved.id, st.file);
          } catch (e) {
            toast(`Документ сохранён, файл нет: ${e.message}`, true);
            location.hash = `#/documents/${saved.id}`;
            return;
          }
        }
        toast("Документ сохранён, форма готова");
        location.hash = `#/documents/${saved.id}`;
      } catch (e) {
        toast(e.message, true);
      }
    });
  };

  draw();
}

function valueControl(field) {
  const v = field.value == null ? "" : field.value;
  if (field.type === "textarea") return `<textarea data-k="value">${esc(v)}</textarea>`;
  if (field.type === "select") {
    const opts = [`<option value="">—</option>`].concat((field.options || []).map((o) => `<option ${o === v ? "selected" : ""}>${esc(o)}</option>`));
    return `<select data-k="value">${opts.join("")}</select>`;
  }
  if (field.type === "checkbox") {
    return `<label class="check"><input data-k="value" type="checkbox" ${v === true ? "checked" : ""}> да</label>`;
  }
  const t = field.type === "number" ? "number" : field.type === "date" ? "date" : "text";
  const step = field.type === "number" ? " step=\"any\"" : "";
  return `<input data-k="value" type="${t}"${step} value="${esc(v)}">`;
}

function collectValues(form, fields) {
  const values = {};
  for (const f of fields) {
    if (f.type === "checkbox") values[f.key] = !!form[f.key]?.checked;
    else if (f.type === "number") {
      const raw = form[f.key]?.value ?? "";
      values[f.key] = raw === "" ? null : Number(raw);
    } else values[f.key] = form[f.key]?.value ?? "";
  }
  return values;
}

async function pageTemplateEditor(view, id) {
  const isNew = !id;
  setNav("templates", isNew ? "Новая форма" : "Форма", "");
  let data = isNew
    ? { name: "", description: "", fields: [blankField()], document_count: 0 }
    : await api(`/api/templates/${id}`);
  if (!data.fields.length) data.fields = [blankField()];

  const draw = () => {
    view.innerHTML = `
      <form class="form form-wide" id="tmpl-form">
        <label><span>Название</span><input name="name" type="text" required value="${esc(data.name)}" placeholder="Приём на работу"></label>
        <label><span>Зачем этот документ</span><textarea name="description" placeholder="Коротко, для людей у стойки">${esc(data.description)}</textarea></label>
        <div>
          <div class="spread"><h2>Поля</h2><button type="button" class="ghost compact" id="add-field">добавить поле</button></div>
          <div class="list" id="fields"></div>
        </div>
        <div class="row">
          <button type="submit">Сохранить</button>
          ${isNew ? "" : `<button type="button" class="ghost" id="new-doc">документ по шаблону</button>`}
          ${isNew ? "" : `<button type="button" class="danger ghost" id="del-tmpl">удалить</button>`}
        </div>
        ${data.document_count ? `<p class="muted">По шаблону уже есть документы (${data.document_count}). Смена полей затронет старые записи: исчезнувшие поля пропадут.</p>` : ""}
      </form>`;
    paintFields();
    bind();
  };

  const paintFields = () => {
    const box = document.getElementById("fields");
    box.innerHTML = data.fields.map((f, i) => `
      <div class="field-card" data-i="${i}">
        <div class="field-grid">
          <label><span>Подпись</span><input data-k="label" type="text" value="${esc(f.label)}" placeholder="ФИО"></label>
          <label><span>Ключ</span><input data-k="key" type="text" value="${esc(f.key)}" placeholder="full_name"></label>
          <label><span>Тип</span>
            <select data-k="type">${TYPES.map(([v, l]) => `<option value="${v}" ${f.type === v ? "selected" : ""}>${l}</option>`).join("")}</select>
          </label>
          <label class="check"><input data-k="required" type="checkbox" ${f.required ? "checked" : ""} ${f.type === "checkbox" ? "disabled" : ""}> обязательно</label>
        </div>
        ${f.type === "select" ? `<label><span>Варианты через запятую</span><input data-k="options" type="text" value="${esc((f.options || []).join(", "))}" placeholder="Трудовой, ГПХ"></label>` : ""}
        <div class="row">
          <button type="button" class="ghost compact" data-up ${i === 0 ? "disabled" : ""}>↑</button>
          <button type="button" class="ghost compact" data-down ${i === data.fields.length - 1 ? "disabled" : ""}>↓</button>
          <button type="button" class="ghost compact" data-rm>убрать</button>
        </div>
      </div>`).join("");
  };

  const readFields = () => {
    view.querySelectorAll(".field-card").forEach((card) => {
      const i = Number(card.dataset.i);
      const f = data.fields[i];
      f.label = card.querySelector('[data-k="label"]').value;
      f.key = card.querySelector('[data-k="key"]').value;
      f.type = card.querySelector('[data-k="type"]').value;
      f.required = card.querySelector('[data-k="required"]').checked;
      const opt = card.querySelector('[data-k="options"]');
      f.options = opt ? opt.value.split(",").map((s) => s.trim()).filter(Boolean) : [];
    });
  };

  const bind = () => {
    view.querySelectorAll('[data-k="label"]').forEach((input) => {
      input.addEventListener("input", () => {
        const card = input.closest(".field-card");
        const key = card.querySelector('[data-k="key"]');
        if (!key.dataset.touched) key.value = slugify(input.value);
      });
    });
    view.querySelectorAll('[data-k="key"]').forEach((input) => {
      input.addEventListener("input", () => { input.dataset.touched = "1"; });
    });
    view.querySelectorAll('[data-k="type"]').forEach((sel) => {
      sel.addEventListener("change", () => {
        readFields();
        draw();
      });
    });
    view.querySelectorAll("[data-up]").forEach((btn) => btn.addEventListener("click", () => {
      const i = Number(btn.closest(".field-card").dataset.i);
      readFields();
      [data.fields[i - 1], data.fields[i]] = [data.fields[i], data.fields[i - 1]];
      draw();
    }));
    view.querySelectorAll("[data-down]").forEach((btn) => btn.addEventListener("click", () => {
      const i = Number(btn.closest(".field-card").dataset.i);
      readFields();
      [data.fields[i + 1], data.fields[i]] = [data.fields[i], data.fields[i + 1]];
      draw();
    }));
    view.querySelectorAll("[data-rm]").forEach((btn) => btn.addEventListener("click", () => {
      const i = Number(btn.closest(".field-card").dataset.i);
      readFields();
      data.fields.splice(i, 1);
      if (!data.fields.length) data.fields.push(blankField());
      draw();
    }));
    document.getElementById("add-field").addEventListener("click", () => {
      readFields();
      data.fields.push(blankField());
      draw();
    });
    document.getElementById("tmpl-form").addEventListener("submit", async (ev) => {
      ev.preventDefault();
      readFields();
      const body = {
        name: ev.target.name.value,
        description: ev.target.description.value,
        fields: data.fields.map((f) => ({
          id: f.id || undefined,
          key: f.key || slugify(f.label),
          label: f.label,
          type: f.type,
          required: f.required,
          options: f.options || [],
        })),
      };
      try {
        const saved = isNew
          ? await api("/api/templates", { method: "POST", body })
          : await api(`/api/templates/${id}`, { method: "PUT", body });
        toast("Форма сохранена");
        location.hash = `#/templates/${saved.id}`;
        if (!isNew && saved.id === id) {
          data = saved;
          draw();
        }
      } catch (e) {
        toast(e.message, true);
      }
    });
    document.getElementById("del-tmpl")?.addEventListener("click", async () => {
      if (!confirm("Удалить форму? Документы по ней должны отсутствовать.")) return;
      try {
        await api(`/api/templates/${id}`, { method: "DELETE" });
        toast("Форма удалена");
        location.hash = "#/templates";
      } catch (e) {
        toast(e.message, true);
      }
    });
    document.getElementById("new-doc")?.addEventListener("click", () => {
      location.hash = `#/import?template=${id}`;
    });
  };

  draw();
}

async function pageDocuments(view, templateId) {
  setNav("documents", "Документы", `<a class="btn" href="#/import">Импорт</a>`);
  const [docs, templates] = await Promise.all([
    api(templateId ? `/api/documents?template_id=${encodeURIComponent(templateId)}` : "/api/documents"),
    api("/api/templates"),
  ]);
  view.innerHTML = `
    <div class="row" style="margin-bottom:16px">
      <label style="max-width:320px"><span>Шаблон</span>
        <select id="flt">
          <option value="">все</option>
          ${templates.map((t) => `<option value="${esc(t.id)}" ${t.id === templateId ? "selected" : ""}>${esc(t.name)}</option>`).join("")}
        </select>
      </label>
    </div>
    ${docs.length === 0 ? `<div class="card empty"><h2>Документов нет</h2><p>Импортируйте первый — поля укажете в редакторе, форма соберётся сама.</p></div>` : `
      <div class="card" style="padding:8px 16px">
        <table class="table">
          <thead><tr><th>Документ</th><th>Форма</th><th>Изменён</th></tr></thead>
          <tbody>
            ${docs.map((d) => `<tr><td><a href="#/documents/${d.id}">${esc(d.title)}</a>${d.has_source ? ` <span class="badge">файл</span>` : ""}</td><td>${esc(d.template_name)}</td><td class="muted">${esc(fmtDate(d.updated_at))}</td></tr>`).join("")}
          </tbody>
        </table>
      </div>`}`;
  document.getElementById("flt").addEventListener("change", (e) => {
    location.hash = e.target.value ? `#/documents?template=${e.target.value}` : "#/documents";
  });
}

function fieldControl(field, value) {
  const v = value == null ? "" : value;
  if (field.type === "textarea") {
    return `<textarea name="${esc(field.key)}">${esc(v)}</textarea>`;
  }
  if (field.type === "select") {
    const opts = [`<option value="">—</option>`]
      .concat((field.options || []).map((o) => `<option ${o === v ? "selected" : ""}>${esc(o)}</option>`));
    return `<select name="${esc(field.key)}">${opts.join("")}</select>`;
  }
  if (field.type === "checkbox") {
    return `<label class="check"><input name="${esc(field.key)}" type="checkbox" ${v === true ? "checked" : ""}> да</label>`;
  }
  const t = field.type === "number" ? "number" : field.type === "date" ? "date" : "text";
  const step = field.type === "number" ? ` step="any"` : "";
  return `<input name="${esc(field.key)}" type="${t}"${step} value="${esc(v)}">`;
}

async function pageDocumentEditor(view, id, templateQuery) {
  const isNew = !id;
  if (isNew) {
    const templates = await api("/api/templates");
    if (templates.length === 0) {
      location.hash = "#/import";
      return;
    }
    let templateId = templateQuery || templates[0].id;
    const loadT = async (tid) => {
      const t = await api(`/api/templates/${tid}`);
      setNav("documents", "Новый документ", "");
      view.innerHTML = `
        <form class="form" id="doc-form">
          <label><span>Форма</span>
            <select id="tmpl-pick">${templates.map((x) => `<option value="${esc(x.id)}" ${x.id === t.id ? "selected" : ""}>${esc(x.name)}</option>`).join("")}</select>
          </label>
          <label><span>Название записи</span><input name="title" type="text" required placeholder="Иванов И. И. — приём"></label>
          <label><span>Текст</span><textarea name="body" class="editor-body"></textarea></label>
          ${t.fields.map((f) => `<label><span>${esc(f.label)}${f.required ? " *" : ""}</span>${fieldControl(f, f.type === "checkbox" ? false : "")}</label>`).join("")}
          <div class="row"><button type="submit">Зарегистрировать</button></div>
        </form>`;
      document.getElementById("tmpl-pick").addEventListener("change", (e) => loadT(e.target.value));
      document.getElementById("doc-form").addEventListener("submit", async (ev) => {
        ev.preventDefault();
        try {
          const saved = await api("/api/documents", {
            method: "POST",
            body: collectDoc(ev.target, t, ev.target.title.value, t.id, ev.target.body.value),
          });
          toast("Документ сохранён");
          location.hash = `#/documents/${saved.id}`;
        } catch (e) {
          toast(e.message, true);
        }
      });
    };
    await loadT(templateId);
    return;
  }

  const doc = await api(`/api/documents/${id}`);
  setNav("documents", doc.title, "");
  const sourceBlock = doc.source
    ? `<p class="row"><a class="btn ghost compact" href="/api/documents/${doc.id}/source">скачать ${esc(doc.source.name)}</a><button type="button" class="ghost compact" id="rm-file">убрать файл</button></p>`
    : `<label class="drop" id="drop"><input type="file" id="file"><strong>Приложить файл</strong><p class="muted">необязательно</p></label>`;
  view.innerHTML = `
    <form class="form" id="doc-form">
      <p class="muted">Форма: <a href="#/templates/${doc.template.id}">${esc(doc.template.name)}</a></p>
      ${sourceBlock}
      <label><span>Название записи</span><input name="title" type="text" required value="${esc(doc.title)}"></label>
      <label><span>Текст</span><textarea name="body" class="editor-body">${esc(doc.body || "")}</textarea></label>
      ${doc.template.fields.map((f) => `<label><span>${esc(f.label)}${f.required ? " *" : ""}</span>${fieldControl(f, doc.values[f.key])}</label>`).join("")}
      <div class="row">
        <button type="submit">Сохранить</button>
        <button type="button" class="danger ghost" id="del-doc">удалить</button>
      </div>
    </form>`;
  document.getElementById("file")?.addEventListener("change", async (e) => {
    const file = e.target.files[0];
    if (!file) return;
    try {
      await uploadSource(id, file);
      toast("Файл приложен");
      render();
    } catch (err) {
      toast(err.message, true);
    }
  });
  document.getElementById("rm-file")?.addEventListener("click", async () => {
    try {
      await api(`/api/documents/${id}/source`, { method: "DELETE", body: {} });
      render();
    } catch (err) {
      toast(err.message, true);
    }
  });
  document.getElementById("doc-form").addEventListener("submit", async (ev) => {
    ev.preventDefault();
    try {
      const body = collectDoc(ev.target, doc.template, ev.target.title.value, doc.template.id, ev.target.body.value);
      const saved = await api(`/api/documents/${id}`, { method: "PUT", body: { title: body.title, body: body.body, values: body.values } });
      document.getElementById("page-title").textContent = saved.title;
      toast("Сохранено");
    } catch (e) {
      toast(e.message, true);
    }
  });
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

function collectDoc(form, template, title, templateId, body = "") {
  return {
    template_id: templateId,
    title,
    body,
    values: collectValues(form, template.fields),
  };
}

async function pageModules(view) {
  setNav("modules", "Модули", "");
  const items = await api("/api/modules");
  view.innerHTML = `<div class="list">${items.map((m) => `
    <div class="card">
      <div class="spread">
        <div>
          <div class="row">
            <h3>${esc(m.name)}</h3>
            ${m.available ? "" : `<span class="badge warn">скоро</span>`}
            ${m.enabled ? `<span class="badge">включён</span>` : ""}
          </div>
          <p class="muted">${esc(m.description)}</p>
        </div>
        <button type="button" class="switch ${m.enabled ? "on" : ""}" data-id="${esc(m.id)}" data-on="${m.enabled ? "1" : "0"}" aria-label="переключить"></button>
      </div>
    </div>`).join("")}
    <p class="muted">Галочка запоминается сейчас. Когда модуль появится в коде, он просто заработает у тех, кто его уже включил.</p>`;
  view.querySelectorAll(".switch").forEach((btn) => {
    btn.addEventListener("click", async () => {
      const on = btn.dataset.on === "1";
      try {
        const path = on ? "disable" : "enable";
        await api(`/api/modules/${btn.dataset.id}/${path}`, { method: "POST", body: {} });
        render();
      } catch (e) {
        toast(e.message, true);
      }
    });
  });
}

window.addEventListener("hashchange", render);
render();
