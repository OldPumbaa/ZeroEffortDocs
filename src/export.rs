use std::io::{Cursor, Write};
use std::path::PathBuf;
use std::time::Duration;

use zip::write::SimpleFileOptions;

use crate::error::AppError;
use crate::layout::{self, Block};
use crate::model::Field;

pub fn layout_to_docx(
    body: &str,
    fields: &[Field],
    values: &serde_json::Map<String, serde_json::Value>,
) -> Result<Vec<u8>, AppError> {
    let html = layout::fill_placeholders(&layout::to_html(body), fields, values);
    let blocks = if let Some(layout) = layout::parse(body) {
        layout.blocks
    } else {
        Vec::new()
    };
    let mut inner = String::new();
    if blocks.is_empty() {
        inner.push_str(&html_fragment_to_paragraphs(
            &html,
            14,
            "Times New Roman",
            "left",
            0,
        ));
    } else {
        for b in &blocks {
            inner.push_str(&block_to_xml(b, fields, values));
        }
    }
    pack_docx(&inner)
}

fn block_to_xml(
    b: &Block,
    fields: &[Field],
    values: &serde_json::Map<String, serde_json::Value>,
) -> String {
    let font = b.font.as_deref().unwrap_or("Times New Roman");
    let size = b.size.unwrap_or(14);
    let align = b.align.as_deref().unwrap_or("left");
    let indent = b.indent.unwrap_or(0);
    let cells: Vec<String> = layout::block_cells(b)
        .into_iter()
        .map(|c| layout::fill_placeholders(&c, fields, values))
        .collect();
    let n = cells.len().clamp(1, 4);
    if n == 1 {
        html_fragment_to_paragraphs(&cells[0], size, font, align, indent)
    } else {
        let mut xml = String::from(
            r#"<w:tbl><w:tblPr><w:tblW w:w="5000" w:type="pct"/><w:tblBorders><w:top w:val="none" w:sz="0"/><w:left w:val="none" w:sz="0"/><w:bottom w:val="none" w:sz="0"/><w:right w:val="none" w:sz="0"/><w:insideH w:val="none" w:sz="0"/><w:insideV w:val="none" w:sz="0"/></w:tblBorders><w:tblCellMar><w:top w:w="0" w:type="dxa"/><w:left w:w="0" w:type="dxa"/><w:bottom w:w="0" w:type="dxa"/><w:right w:w="80" w:type="dxa"/></w:tblCellMar></w:tblPr><w:tr>"#,
        );
        let w = 5000 / n as i32;
        for (i, cell) in cells.iter().enumerate() {
            let cell_align = if n == 2 && i + 1 == n { "right" } else { align };
            xml.push_str(&format!(
                r#"<w:tc><w:tcPr><w:tcW w:w="{w}" w:type="pct"/><w:vAlign w:val="top"/></w:tcPr>{p}</w:tc>"#,
                p = html_fragment_to_paragraphs(cell, size, font, cell_align, 0)
            ));
        }
        xml.push_str("</w:tr></w:tbl>");
        xml
    }
}

fn html_fragment_to_paragraphs(
    html: &str,
    size: u32,
    font: &str,
    align: &str,
    indent: u32,
) -> String {
    let jc = match align {
        "center" => "center",
        "right" => "right",
        "both" | "justify" => "both",
        _ => "left",
    };
    let indent_xml = if indent > 0 {
        let twips = (indent as i32) * 709;
        format!(r#"<w:ind w:firstLine="{twips}"/>"#)
    } else {
        String::new()
    };
    let runs = html_to_runs(html, size, font);
    format!(
        r#"<w:p><w:pPr><w:jc w:val="{jc}"/>{indent_xml}<w:spacing w:before="0" w:after="80" w:line="276" w:lineRule="auto"/></w:pPr>{runs}</w:p>"#
    )
}

#[derive(Clone)]
struct RunStyle {
    bold: bool,
    italic: bool,
    underline: bool,
    size: u32,
    font: String,
}

fn html_to_runs(html: &str, size: u32, font: &str) -> String {
    let mut out = String::new();
    let mut stack = vec![RunStyle {
        bold: false,
        italic: false,
        underline: false,
        size,
        font: font.to_string(),
    }];
    let mut i = 0;
    let chars: Vec<char> = html.chars().collect();
    while i < chars.len() {
        if chars[i] == '<' {
            let rest: String = chars[i..].iter().collect();
            if let Some(end) = rest.find('>') {
                let tag = rest[1..end].trim().to_ascii_lowercase();
                let name = tag
                    .split_whitespace()
                    .next()
                    .unwrap_or("")
                    .trim_start_matches('/');
                let closing = tag.starts_with('/');
                if name == "br" {
                    out.push_str("<w:r><w:br/></w:r>");
                } else if closing {
                    if stack.len() > 1 {
                        stack.pop();
                    }
                } else {
                    let mut st = stack.last().cloned().unwrap();
                    if name == "b" || name == "strong" {
                        st.bold = true;
                    } else if name == "i" || name == "em" {
                        st.italic = true;
                    } else if name == "u" {
                        st.underline = true;
                    } else if name == "span" || name == "font" {
                        apply_css_style(&mut st, &rest);
                    }
                    stack.push(st);
                }
                i += end + 1;
                continue;
            }
        }
        let mut text = String::new();
        while i < chars.len() && chars[i] != '<' {
            text.push(chars[i]);
            i += 1;
        }
        let text = decode_entities(&text);
        if text.is_empty() {
            continue;
        }
        let st = stack.last().cloned().unwrap();
        out.push_str(&run_xml(&text, &st));
    }
    if out.is_empty() {
        out.push_str(&run_xml(" ", &stack[0]));
    }
    out
}

fn apply_css_style(st: &mut RunStyle, tag: &str) {
    let lower = tag.to_ascii_lowercase();
    if lower.contains("font-weight:bold")
        || lower.contains("font-weight: bold")
        || lower.contains("font-weight:700")
        || lower.contains("font-weight: 700")
        || lower.contains("font-weight:600")
    {
        st.bold = true;
    }
    if lower.contains("font-style:italic") || lower.contains("font-style: italic") {
        st.italic = true;
    }
    if lower.contains("underline") {
        st.underline = true;
    }
    if let Some(pt) = lower.find("font-size:") {
        let slice = &lower[pt + 10..];
        if let Ok(num) = slice
            .chars()
            .skip_while(|c| !c.is_ascii_digit())
            .take_while(|c| c.is_ascii_digit())
            .collect::<String>()
            .parse::<u32>()
        {
            if (1..80).contains(&num) {
                st.size = num;
            }
        }
    }
}

fn run_xml(text: &str, st: &RunStyle) -> String {
    let b = if st.bold { "<w:b/><w:bCs/>" } else { "" };
    let i = if st.italic { "<w:i/><w:iCs/>" } else { "" };
    let u = if st.underline {
        r#"<w:u w:val="single"/>"#
    } else {
        ""
    };
    let sz = st.size * 2;
    let font = xml_escape(&st.font);
    format!(
        r#"<w:r><w:rPr>{b}{i}{u}<w:sz w:val="{sz}"/><w:szCs w:val="{sz}"/><w:rFonts w:ascii="{font}" w:hAnsi="{font}" w:cs="{font}"/></w:rPr><w:t xml:space="preserve">{}</w:t></w:r>"#,
        xml_escape(text)
    )
}

fn decode_entities(s: &str) -> String {
    s.replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn pack_docx(body_inner: &str) -> Result<Vec<u8>, AppError> {
    let document = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>{body_inner}<w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="1134" w:right="850" w:bottom="1134" w:left="1134"/></w:sectPr></w:body></w:document>"#
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

pub fn html_to_pdf(html: &str) -> Result<Vec<u8>, AppError> {
    let soffice = find_soffice().ok_or_else(|| {
        AppError::bad(
            "LibreOffice не найден. Поставьте Writer или распечатайте предпросмотр в PDF из браузера",
        )
    })?;
    let dir = std::env::temp_dir().join(format!("zed-pdf-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir)?;
    let html_path = dir.join("doc.html");
    std::fs::write(&html_path, html.as_bytes())?;
    let status = std::process::Command::new(&soffice)
        .args([
            "--headless",
            "--norestore",
            "--convert-to",
            "pdf",
            "--outdir",
        ])
        .arg(&dir)
        .arg(&html_path)
        .status()
        .map_err(|e| AppError::bad(format!("не удалось запустить LibreOffice: {e}")))?;
    if !status.success() {
        let _ = std::fs::remove_dir_all(&dir);
        return Err(AppError::bad("LibreOffice не смог собрать PDF"));
    }
    let pdf_path = dir.join("doc.pdf");
    let started = std::time::Instant::now();
    while !pdf_path.exists() && started.elapsed() < Duration::from_secs(20) {
        std::thread::sleep(Duration::from_millis(150));
    }
    let bytes = std::fs::read(&pdf_path);
    let _ = std::fs::remove_dir_all(&dir);
    bytes.map_err(|_| {
        AppError::bad("PDF не появился. Если открыт Writer, закройте его и попробуйте снова")
    })
}

fn find_soffice() -> Option<PathBuf> {
    let candidates = [
        r"C:\Program Files\LibreOffice\program\soffice.exe",
        r"C:\Program Files (x86)\LibreOffice\program\soffice.exe",
        r"C:\Program Files\LibreOffice\program\soffice.com",
    ];
    for c in candidates {
        let p = PathBuf::from(c);
        if p.exists() {
            return Some(p);
        }
    }
    which("soffice").or_else(|| which("soffice.exe"))
}

fn which(name: &str) -> Option<PathBuf> {
    let paths = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&paths) {
        let p = dir.join(name);
        if p.exists() {
            return Some(p);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn docx_from_layout_has_text() {
        let raw = r#"{"v":1,"blocks":[{"type":"paragraph","align":"center","html":"<b>ПРИКАЗ</b>","size":18}]}"#;
        let bytes = layout_to_docx(raw, &[], &serde_json::Map::new()).unwrap();
        let extracted = crate::extract::from_bytes("t.docx", &bytes).unwrap();
        assert!(extracted.text.contains("ПРИКАЗ"), "{}", extracted.text);
    }

    #[test]
    fn docx_keeps_bold_span() {
        use std::io::Read;
        let raw = r#"{"v":1,"blocks":[{"type":"block","html":"<span style=\"font-weight:bold\">ЖИРНЫЙ</span>"}]}"#;
        let bytes = layout_to_docx(raw, &[], &serde_json::Map::new()).unwrap();
        let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
        let mut file = zip.by_name("word/document.xml").unwrap();
        let mut xml = String::new();
        file.read_to_string(&mut xml).unwrap();
        assert!(xml.contains("<w:b"), "{xml}");
        assert!(xml.contains("<w:bCs"), "{xml}");
        assert!(xml.contains("ЖИРНЫЙ"), "{xml}");
    }
}
