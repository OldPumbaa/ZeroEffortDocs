use std::io::{Cursor, Read};

use crate::error::AppError;

pub fn docx_to_html(bytes: &[u8]) -> Result<String, AppError> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes))?;
    let mut file = zip
        .by_name("word/document.xml")
        .map_err(|_| AppError::bad("в файле нет word/document.xml"))?;
    let mut xml = String::new();
    file.read_to_string(&mut xml)?;
    Ok(document_xml_to_html(&xml))
}

pub fn wrap_preview_page(title: &str, inner: &str) -> String {
    wrap_sheet(title, inner, 1, false)
}

pub fn wrap_print_page(title: &str, inner: &str, pages: u32) -> String {
    wrap_sheet(title, inner, pages.clamp(1, 5), true)
}

fn wrap_sheet(title: &str, inner: &str, pages: u32, for_pdf: bool) -> String {
    let title_esc = esc(title);
    let height = 297 * pages;
    let chrome = if for_pdf {
        "html, body { margin: 0; background: #fff; color: #000; }"
    } else {
        "html, body { margin: 0; background: #fff; color: #241c15; }"
    };
    format!(
        r#"<!DOCTYPE html>
<html lang="ru">
<head>
<meta charset="utf-8">
<title>{title_esc}</title>
<style>
  {chrome}
  @page {{ size: A4; margin: 0; }}
  .page {{
    width: 210mm;
    height: {height}mm;
    overflow: hidden;
    margin: 0 auto;
    background: #fff;
    padding: 18mm 16mm;
    box-sizing: border-box;
    font-family: "Times New Roman", Times, "PT Serif", serif;
    font-size: 14pt;
    line-height: 1.25;
  }}
  p {{ margin: 0 0 0.25em; min-height: 1em; }}
  table {{ border-collapse: collapse; width: 100%; margin: 0.2em 0; }}
  td, th {{ border: none; padding: 0 6px 0 0; vertical-align: top; }}
  .zed-header td {{ vertical-align: top; }}
</style>
</head>
<body><div class="page">{inner}</div></body>
</html>"#
    )
}

fn document_xml_to_html(xml: &str) -> String {
    use quick_xml::events::Event;
    use quick_xml::Reader;

    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut buf = Vec::new();
    let mut html = String::new();
    let mut in_t = false;
    let mut in_p_pr = false;
    let mut in_r_pr = false;
    let mut in_r = false;
    let mut p_opened = false;
    let mut p_align = String::new();
    let mut bold = false;
    let mut italic = false;
    let mut underline = false;
    let mut sz: Option<u32> = None;
    let mut color: Option<String> = None;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Empty(e)) => match e.local_name().as_ref() {
                b"p" => html.push_str("<p>&nbsp;</p>"),
                b"tab" => {
                    if !p_opened {
                        open_p(&mut html, &p_align);
                        p_opened = true;
                    }
                    html.push_str("&emsp;");
                }
                b"br" | b"cr" => html.push_str("<br>"),
                b"b" if in_r_pr => bold = attr_on(&e),
                b"i" if in_r_pr => italic = attr_on(&e),
                b"u" if in_r_pr => underline = attr(&e, b"val").as_deref() != Some("none"),
                b"sz" if in_r_pr => {
                    if let Some(v) = attr(&e, b"val") {
                        sz = v.parse().ok();
                    }
                }
                b"color" if in_r_pr => {
                    if let Some(v) = attr(&e, b"val") {
                        if v != "auto" && v.len() == 6 {
                            color = Some(v);
                        }
                    }
                }
                b"jc" if in_p_pr => {
                    if let Some(val) = attr(&e, b"val") {
                        p_align = match val.as_str() {
                            "center" => "text-align:center;".into(),
                            "right" => "text-align:right;".into(),
                            "both" => "text-align:justify;".into(),
                            _ => String::new(),
                        };
                    }
                }
                _ => {}
            },
            Ok(Event::Start(e)) => {
                let name = e.local_name();
                match name.as_ref() {
                    b"tbl" => html.push_str("<table>"),
                    b"tr" => html.push_str("<tr>"),
                    b"tc" => html.push_str("<td>"),
                    b"p" => {
                        p_opened = false;
                        p_align.clear();
                        in_p_pr = false;
                    }
                    b"pPr" => in_p_pr = true,
                    b"jc" if in_p_pr => {
                        if let Some(val) = attr(&e, b"val") {
                            p_align = match val.as_str() {
                                "center" => "text-align:center;".into(),
                                "right" => "text-align:right;".into(),
                                "both" => "text-align:justify;".into(),
                                _ => String::new(),
                            };
                        }
                    }
                    b"r" => {
                        if !p_opened {
                            open_p(&mut html, &p_align);
                            p_opened = true;
                        }
                        in_r = true;
                        in_r_pr = false;
                        bold = false;
                        italic = false;
                        underline = false;
                        sz = None;
                        color = None;
                    }
                    b"rPr" if in_r => in_r_pr = true,
                    b"b" if in_r_pr => bold = attr_on(&e),
                    b"i" if in_r_pr => italic = attr_on(&e),
                    b"u" if in_r_pr => underline = attr(&e, b"val").as_deref() != Some("none"),
                    b"sz" if in_r_pr => {
                        if let Some(v) = attr(&e, b"val") {
                            sz = v.parse().ok();
                        }
                    }
                    b"color" if in_r_pr => {
                        if let Some(v) = attr(&e, b"val") {
                            if v != "auto" && v.len() == 6 {
                                color = Some(v);
                            }
                        }
                    }
                    b"t" => in_t = true,
                    b"tab" => {
                        if !p_opened {
                            open_p(&mut html, &p_align);
                            p_opened = true;
                        }
                        html.push_str("&emsp;");
                    }
                    b"br" | b"cr" => html.push_str("<br>"),
                    _ => {}
                }
            }
            Ok(Event::End(e)) => match e.local_name().as_ref() {
                b"tbl" => html.push_str("</table>"),
                b"tr" => html.push_str("</tr>"),
                b"tc" => html.push_str("</td>"),
                b"pPr" => in_p_pr = false,
                b"rPr" => in_r_pr = false,
                b"r" => in_r = false,
                b"t" => in_t = false,
                b"p" => {
                    if !p_opened {
                        html.push_str("<p>&nbsp;</p>");
                    } else {
                        html.push_str("</p>");
                    }
                    p_opened = false;
                }
                _ => {}
            },
            Ok(Event::Text(t)) if in_t => {
                if !p_opened {
                    open_p(&mut html, &p_align);
                    p_opened = true;
                }
                if let Ok(s) = t.unescape() {
                    let mut span = String::new();
                    if bold {
                        span.push_str("font-weight:bold;");
                    }
                    if italic {
                        span.push_str("font-style:italic;");
                    }
                    if underline {
                        span.push_str("text-decoration:underline;");
                    }
                    if let Some(sz) = sz {
                        span.push_str(&format!("font-size:{}pt;", sz as f32 / 2.0));
                    }
                    if let Some(c) = &color {
                        span.push_str(&format!("color:#{c};"));
                    }
                    let text = esc(&s).replace('\n', "<br>");
                    if span.is_empty() {
                        html.push_str(&text);
                    } else {
                        html.push_str(&format!("<span style=\"{span}\">{text}</span>"));
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    html
}

fn open_p(html: &mut String, align: &str) {
    if align.is_empty() {
        html.push_str("<p>");
    } else {
        html.push_str("<p style=\"");
        html.push_str(align);
        html.push_str("\">");
    }
}

fn attr_on(e: &quick_xml::events::BytesStart<'_>) -> bool {
    !matches!(
        attr(e, b"val").as_deref(),
        Some("0") | Some("false") | Some("off")
    )
}

fn attr(e: &quick_xml::events::BytesStart<'_>, key: &[u8]) -> Option<String> {
    e.attributes().with_checks(false).find_map(|a| {
        let a = a.ok()?;
        if a.key.local_name().as_ref() == key {
            Some(String::from_utf8_lossy(&a.value).into_owned())
        } else {
            None
        }
    })
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bold_center() {
        let xml = r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>
          <w:p><w:pPr><w:jc w:val="center"/></w:pPr>
            <w:r><w:rPr><w:b/><w:sz w:val="32"/></w:rPr><w:t>Заголовок</w:t></w:r>
          </w:p>
          <w:p><w:r><w:t>Текст {{fio}}</w:t></w:r></w:p>
        </w:body></w:document>"#;
        let html = document_xml_to_html(xml);
        assert!(html.contains("text-align:center"), "{html}");
        assert!(html.contains("font-weight:bold"), "{html}");
        assert!(html.contains("16pt"), "{html}");
        assert!(html.contains("Заголовок"), "{html}");
        assert!(html.contains("Текст {{fio}}"), "{html}");
    }
}
