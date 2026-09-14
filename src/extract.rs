use std::io::{Cursor, Read, Write};

use serde::Serialize;
use serde_json::{Map, Value};
use zip::write::SimpleFileOptions;

use crate::error::AppError;
use crate::files;
use crate::model::{is_valid_key, FieldType, FillMode};

#[derive(Debug, Clone, Serialize)]
pub struct ExtractedField {
    pub key: String,
    pub label: String,
    #[serde(rename = "type")]
    pub field_type: FieldType,
    pub required: bool,
    pub fill_mode: FillMode,
}

#[derive(Debug, Clone, Serialize)]
pub struct Extracted {
    pub text: String,
    pub fields: Vec<ExtractedField>,
    pub format: String,
}

pub fn from_bytes(filename: &str, bytes: &[u8]) -> Result<Extracted, AppError> {
    let ext = files::check(filename, bytes)?;
    let (raw, format) = match ext.as_str() {
        "txt" | "md" | "" => (String::from_utf8_lossy(bytes).into_owned(), "txt"),
        "docx" => (docx_text(bytes)?, "docx"),
        "doc" => {
            return Err(AppError::bad(
                "старый .doc не читаем. Сохраните в Word как .docx или поставьте {{переменные}} в новом файле",
            ));
        }
        "pdf" => {
            return Err(AppError::bad(
                "PDF пока не разбираем. Сохраните как .docx с {{переменными}} или вставьте текст на лист",
            ));
        }
        other => {
            return Err(AppError::bad(format!(
                "формат .{other} на лист не кладётся. Нужен .docx или .txt"
            )));
        }
    };
    Ok(build_extracted(&raw, format))
}

fn build_extracted(raw: &str, format: &str) -> Extracted {
    let marks = collect_marks(raw);
    let mut text = raw.to_string();
    let mut fields = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for (original, key, label) in marks {
        if !seen.insert(key.clone()) {
            continue;
        }
        if original != key {
            text = text.replace(&format!("{{{{{original}}}}}"), &format!("{{{{{key}}}}}"));
        }
        let fill_mode = guess_fill(&key);
        let field_type = match fill_mode {
            FillMode::CreatedAt => FieldType::Date,
            FillMode::Sequence => FieldType::Number,
            FillMode::Manual => FieldType::Text,
        };
        fields.push(ExtractedField {
            key,
            label,
            field_type,
            required: fill_mode == FillMode::Manual,
            fill_mode,
        });
    }
    Extracted {
        text,
        fields,
        format: format.to_string(),
    }
}

pub fn fill_docx(bytes: &[u8], values: &Map<String, Value>) -> Result<Vec<u8>, AppError> {
    let reps: Vec<(String, String)> = values
        .iter()
        .map(|(k, v)| (k.clone(), value_text(v)))
        .collect();
    rewrite_zip(bytes, |xml| fill_xml(xml, &reps))
}

pub fn normalize_docx(bytes: &[u8]) -> Result<Vec<u8>, AppError> {
    rewrite_zip(bytes, normalize_xml)
}

pub fn docx_from_text(text: &str) -> Result<Vec<u8>, AppError> {
    let mut paras = String::new();
    for line in text.split('\n') {
        paras.push_str("<w:p><w:r><w:t xml:space=\"preserve\">");
        paras.push_str(&xml_escape(line));
        paras.push_str("</w:t></w:r></w:p>");
    }
    if paras.is_empty() {
        paras.push_str("<w:p/>");
    }
    let document = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>{paras}</w:body></w:document>"#
    );
    let types = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#;
    let rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;

    let mut inner = Cursor::new(Vec::new());
    let mut zip = zip::ZipWriter::new(&mut inner);
    let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    zip.start_file("[Content_Types].xml", opts)?;
    zip.write_all(types.as_bytes())?;
    zip.start_file("_rels/.rels", opts)?;
    zip.write_all(rels.as_bytes())?;
    zip.start_file("word/document.xml", opts)?;
    zip.write_all(document.as_bytes())?;
    zip.finish()?;
    Ok(inner.into_inner())
}

fn value_text(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(true) => "да".into(),
        Value::Bool(false) => "нет".into(),
        other => other.to_string(),
    }
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn xml_unescape(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
}

fn should_rewrite(name: &str) -> bool {
    let n = name.replace('\\', "/");
    n.starts_with("word/") && n.ends_with(".xml") && !n.contains("/_rels/")
}

fn rewrite_zip(bytes: &[u8], mut rewrite: impl FnMut(&str) -> String) -> Result<Vec<u8>, AppError> {
    let mut input = zip::ZipArchive::new(Cursor::new(bytes))?;
    let mut entries = Vec::new();
    for i in 0..input.len() {
        let mut file = input.by_index(i)?;
        let name = file.name().to_string();
        if name.ends_with('/') {
            continue;
        }
        let method = file.compression();
        let mut data = Vec::new();
        file.read_to_end(&mut data)?;
        entries.push((name, method, data));
    }
    drop(input);

    let mut out = Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut out);
        for (name, method, data) in entries {
            let opts = SimpleFileOptions::default().compression_method(method);
            let data = if should_rewrite(&name) {
                let xml = String::from_utf8_lossy(&data);
                rewrite(&xml).into_bytes()
            } else {
                data
            };
            zip.start_file(&name, opts)?;
            zip.write_all(&data)?;
        }
        zip.finish()?;
    }
    Ok(out.into_inner())
}

fn fill_xml(xml: &str, reps: &[(String, String)]) -> String {
    map_paragraphs(xml, |joined| {
        let mut t = joined;
        for (k, v) in reps {
            t = t.replace(&format!("{{{{{k}}}}}"), v);
        }
        t
    })
}

fn normalize_xml(xml: &str) -> String {
    map_paragraphs(xml, |joined| {
        let mut t = joined;
        for (original, key, _) in collect_marks(&t) {
            if original != key {
                t = t.replace(&format!("{{{{{original}}}}}"), &format!("{{{{{key}}}}}"));
            }
        }
        t
    })
}

fn map_paragraphs(xml: &str, mut map: impl FnMut(String) -> String) -> String {
    let mut out = String::new();
    let mut rest = xml;
    while let Some(start) = find_para_start(rest) {
        out.push_str(&rest[..start]);
        let after = &rest[start..];
        let Some(end) = after.find("</w:p>").map(|e| e + 6) else {
            out.push_str(after);
            return out;
        };
        let para = &after[..end];
        out.push_str(&rewrite_paragraph(para, map(extract_wt_texts(para))));
        rest = &after[end..];
    }
    out.push_str(rest);
    out
}

fn find_para_start(s: &str) -> Option<usize> {
    let mut search = 0;
    while let Some(i) = s[search..].find("<w:p") {
        let abs = search + i;
        let next = s[abs + 4..].chars().next();
        if matches!(next, Some(' ' | '>' | '/')) {
            return Some(abs);
        }
        search = abs + 4;
    }
    None
}

fn extract_wt_texts(para: &str) -> String {
    let mut out = String::new();
    let mut rest = para;
    while let Some(start) = rest.find("<w:t") {
        let tag = &rest[start..];
        let Some(gt) = tag.find('>') else { break };
        if tag[..gt].ends_with('/') {
            rest = &tag[gt + 1..];
            continue;
        }
        let content = &tag[gt + 1..];
        let Some(end) = content.find("</w:t>") else {
            break;
        };
        out.push_str(&xml_unescape(&content[..end]));
        rest = &content[end + 6..];
    }
    out
}

fn rewrite_paragraph(para: &str, new_text: String) -> String {
    let old = extract_wt_texts(para);
    if old == new_text || !para.contains("<w:t") {
        return para.to_string();
    }
    let escaped = xml_escape(&new_text);
    let mut out = String::new();
    let mut rest = para;
    let mut first = true;
    while let Some(start) = rest.find("<w:t") {
        out.push_str(&rest[..start]);
        let tag = &rest[start..];
        let Some(gt) = tag.find('>') else {
            out.push_str(rest);
            return out;
        };
        if tag[..gt].ends_with('/') {
            out.push_str(&tag[..=gt]);
            rest = &tag[gt + 1..];
            continue;
        }
        let content = &tag[gt + 1..];
        let Some(end) = content.find("</w:t>") else {
            out.push_str(rest);
            return out;
        };
        if first {
            out.push_str("<w:t xml:space=\"preserve\">");
            out.push_str(&escaped);
            out.push_str("</w:t>");
            first = false;
        } else {
            out.push_str("<w:t></w:t>");
        }
        rest = &content[end + 6..];
    }
    out.push_str(rest);
    out
}

fn collect_marks(text: &str) -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == b'{' && bytes[i + 1] == b'{' {
            if let Some(end) = text[i + 2..].find("}}") {
                let original = text[i + 2..i + 2 + end].trim().to_string();
                i = i + 4 + end;
                if original.is_empty() {
                    continue;
                }
                let key = slugify(&original);
                if !is_valid_key(&key) {
                    continue;
                }
                let label = if original.chars().any(|c| c.is_alphabetic() && !c.is_ascii()) {
                    original.clone()
                } else {
                    original.replace('_', " ")
                };
                out.push((original, key, label));
                continue;
            }
        }
        i += 1;
    }
    out
}

fn guess_fill(key: &str) -> FillMode {
    let k = key.to_ascii_lowercase();
    if k.contains("date")
        || k.contains("data")
        || k == "when"
        || k.ends_with("_at")
        || k.contains("dt")
    {
        FillMode::CreatedAt
    } else if k == "num"
        || k == "nomer"
        || k == "nn"
        || k.ends_with("_no")
        || k.contains("number")
        || k.contains("nomer")
    {
        FillMode::Sequence
    } else {
        FillMode::Manual
    }
}

fn slugify(s: &str) -> String {
    let mut out = String::new();
    for ch in s.chars() {
        let lower = ch.to_lowercase().next().unwrap_or(ch);
        if let Some(rep) = translit(lower) {
            out.push_str(rep);
        } else if lower.is_ascii_alphanumeric() {
            out.push(lower);
        } else {
            out.push('_');
        }
    }
    while out.contains("__") {
        out = out.replace("__", "_");
    }
    let out = out.trim_matches('_').to_string();
    if out.is_empty() {
        return String::new();
    }
    if out.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        format!("f_{out}")
    } else {
        out.chars().take(64).collect()
    }
}

fn translit(c: char) -> Option<&'static str> {
    Some(match c {
        'а' => "a",
        'б' => "b",
        'в' => "v",
        'г' => "g",
        'д' => "d",
        'е' | 'ё' => "e",
        'ж' => "zh",
        'з' => "z",
        'и' => "i",
        'й' => "j",
        'к' => "k",
        'л' => "l",
        'м' => "m",
        'н' => "n",
        'о' => "o",
        'п' => "p",
        'р' => "r",
        'с' => "s",
        'т' => "t",
        'у' => "u",
        'ф' => "f",
        'х' => "h",
        'ц' => "c",
        'ч' => "ch",
        'ш' => "sh",
        'щ' => "sch",
        'ъ' | 'ь' => "",
        'ы' => "y",
        'э' => "e",
        'ю' => "yu",
        'я' => "ya",
        _ => return None,
    })
}

fn docx_text(bytes: &[u8]) -> Result<String, AppError> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes))?;
    let mut file = zip
        .by_name("word/document.xml")
        .map_err(|_| AppError::bad("в файле нет word/document.xml — это не обычный .docx"))?;
    let mut xml = String::new();
    file.read_to_string(&mut xml)?;
    Ok(text_from_document_xml(&xml))
}

fn text_from_document_xml(xml: &str) -> String {
    use quick_xml::events::Event;
    use quick_xml::Reader;

    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut buf = Vec::new();
    let mut out = String::new();
    let mut in_t = false;
    let mut first_p = true;
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                let n = e.local_name();
                match n.as_ref() {
                    b"p" => {
                        if !first_p {
                            out.push('\n');
                        }
                        first_p = false;
                    }
                    b"tab" => out.push('\t'),
                    b"br" | b"cr" => out.push('\n'),
                    b"t" => in_t = true,
                    _ => {}
                }
            }
            Ok(Event::End(e)) => {
                if e.local_name().as_ref() == b"t" {
                    in_t = false;
                }
            }
            Ok(Event::Text(t)) if in_t => {
                if let Ok(s) = t.unescape() {
                    out.push_str(&s);
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Write};
    use zip::write::SimpleFileOptions;

    fn docx_from_paragraphs(parts: &[&str], split_first: bool) -> Vec<u8> {
        let mut inner = Cursor::new(Vec::new());
        let mut zip = zip::ZipWriter::new(&mut inner);
        let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        zip.start_file("word/document.xml", opts).unwrap();
        let mut xml = String::from(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>"#,
        );
        for (i, p) in parts.iter().enumerate() {
            xml.push_str("<w:p>");
            if split_first && i == 0 {
                let mid = p
                    .char_indices()
                    .nth(p.chars().count() / 2)
                    .map(|(idx, _)| idx)
                    .unwrap_or(0);
                xml.push_str("<w:r><w:t>");
                xml.push_str(&escape(&p[..mid]));
                xml.push_str("</w:t></w:r><w:r><w:t>");
                xml.push_str(&escape(&p[mid..]));
                xml.push_str("</w:t></w:r>");
            } else {
                xml.push_str("<w:r><w:t>");
                xml.push_str(&escape(p));
                xml.push_str("</w:t></w:r>");
            }
            xml.push_str("</w:p>");
        }
        xml.push_str("</w:body></w:document>");
        zip.write_all(xml.as_bytes()).unwrap();
        zip.finish().unwrap();
        inner.into_inner()
    }

    fn escape(s: &str) -> String {
        s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
    }

    #[test]
    fn docx_placeholders_and_split_runs() {
        let bytes = docx_from_paragraphs(
            &["Приказ о приёме {{ФИО}}", "Дата {{date}}, номер {{num}}"],
            true,
        );
        let extracted = from_bytes("hire.docx", &bytes).unwrap();
        assert!(extracted.text.contains("{{fio}}"));
        assert!(extracted.text.contains("{{date}}"));
        assert!(extracted.text.contains("{{num}}"));
        let keys: Vec<_> = extracted.fields.iter().map(|f| f.key.as_str()).collect();
        assert_eq!(keys, ["fio", "date", "num"]);
        assert_eq!(extracted.fields[0].label, "ФИО");
        assert_eq!(extracted.fields[1].fill_mode, FillMode::CreatedAt);
        assert_eq!(extracted.fields[2].fill_mode, FillMode::Sequence);
    }

    #[test]
    fn txt_plain() {
        let extracted = from_bytes("a.txt", b"Hello {{full_name}}").unwrap();
        assert_eq!(extracted.fields[0].key, "full_name");
        assert_eq!(extracted.text, "Hello {{full_name}}");
    }

    #[test]
    fn fill_split_placeholder() {
        let bytes = docx_from_paragraphs(&["Сотрудник {{fio}}, приказ {{num}}"], true);
        let mut values = Map::new();
        values.insert("fio".into(), Value::String("Иванов И.И.".into()));
        values.insert("num".into(), serde_json::json!(7));
        let filled = fill_docx(&bytes, &values).unwrap();
        let text = docx_text(&filled).unwrap();
        assert!(text.contains("Иванов И.И."), "{text}");
        assert!(text.contains('7'), "{text}");
        assert!(!text.contains("{{fio}}"), "{text}");
    }

    #[test]
    fn normalize_then_fill() {
        let bytes = docx_from_paragraphs(&["ФИО {{ФИО}}"], false);
        let normalized = normalize_docx(&bytes).unwrap();
        let extracted = from_bytes("a.docx", &normalized).unwrap();
        assert!(extracted.text.contains("{{fio}}"));
        let mut values = Map::new();
        values.insert("fio".into(), Value::String("Петров".into()));
        let filled = fill_docx(&normalized, &values).unwrap();
        let text = docx_text(&filled).unwrap();
        assert!(text.contains("Петров"));
        assert!(!text.contains("{{"));
    }
}
