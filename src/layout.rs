use serde::Deserialize;

use crate::model::Field;

#[derive(Debug, Deserialize)]
pub struct Layout {
    pub v: u32,
    #[serde(default)]
    pub pages: Option<u32>,
    pub blocks: Vec<Block>,
}

#[derive(Debug, Deserialize)]
pub struct Block {
    #[serde(rename = "type")]
    pub kind: String,
    pub align: Option<String>,
    pub font: Option<String>,
    pub size: Option<u32>,
    #[serde(default)]
    pub html: Option<serde_json::Value>,
    pub left: Option<String>,
    pub right: Option<String>,
    #[serde(default)]
    pub indent: Option<u32>,
    #[serde(default)]
    pub cols: Option<u32>,
}

pub fn is_layout_json(body: &str) -> bool {
    let t = body.trim_start();
    t.starts_with('{') && t.contains("\"blocks\"")
}

pub fn is_html(body: &str) -> bool {
    let t = body.trim_start();
    t.starts_with('<')
}

pub fn page_count(body: &str) -> u32 {
    parse(body).and_then(|l| l.pages).unwrap_or(1).clamp(1, 5)
}

pub fn parse(body: &str) -> Option<Layout> {
    let trimmed = body.trim();
    let layout: Layout = serde_json::from_str(trimmed).ok()?;
    if layout.v >= 1 {
        Some(layout)
    } else {
        None
    }
}

pub fn block_cells(b: &Block) -> Vec<String> {
    if let Some(val) = &b.html {
        if let Some(arr) = val.as_array() {
            let mut cells: Vec<String> = arr
                .iter()
                .map(|x| x.as_str().unwrap_or("").to_string())
                .collect();
            let n = col_count(b);
            while cells.len() < n {
                cells.push(String::new());
            }
            cells.truncate(n);
            return cells;
        }
        if let Some(s) = val.as_str() {
            if b.kind == "header" {
                return vec![s.to_string(), b.right.clone().unwrap_or_default()];
            }
            return vec![s.to_string()];
        }
    }
    if b.kind == "header" {
        return vec![
            b.left.clone().unwrap_or_default(),
            b.right.clone().unwrap_or_default(),
        ];
    }
    vec![String::new()]
}

pub fn col_count(b: &Block) -> usize {
    if let Some(c) = b.cols {
        return (c.clamp(1, 4)) as usize;
    }
    if b.kind == "header" {
        2
    } else {
        1
    }
}

/// Turns stored template body (JSON layout or plain text) into HTML with `{{key}}` still in it.
pub fn to_html(body: &str) -> String {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if let Some(layout) = parse(trimmed) {
        return render_blocks(&layout.blocks);
    }
    if is_html(trimmed) {
        return trimmed.to_string();
    }
    body.lines()
        .map(|line| {
            if line.is_empty() {
                "<p>&nbsp;</p>".into()
            } else {
                format!("<p>{}</p>", html_escape(line))
            }
        })
        .collect()
}

pub fn fill_placeholders(
    html: &str,
    fields: &[Field],
    values: &serde_json::Map<String, serde_json::Value>,
) -> String {
    let mut out = html.to_string();
    for field in fields {
        let needle = format!("{{{{{}}}}}", field.key);
        let replacement = match values.get(&field.key) {
            None | Some(serde_json::Value::Null) => String::new(),
            Some(serde_json::Value::String(s)) => {
                if field.field_type == crate::model::FieldType::Date {
                    html_escape(&format_date(s, &field.date_format))
                } else {
                    html_escape(s)
                }
            }
            Some(serde_json::Value::Number(n)) => n.to_string(),
            Some(serde_json::Value::Bool(true)) => "да".into(),
            Some(serde_json::Value::Bool(false)) => "нет".into(),
            Some(other) => html_escape(&other.to_string()),
        };
        out = out.replace(&needle, &replacement);
    }
    out
}

fn render_blocks(blocks: &[Block]) -> String {
    let mut html = String::from(r#"<div class="zed-doc">"#);
    for b in blocks {
        let font = b.font.as_deref().unwrap_or("Times New Roman");
        let size = b.size.unwrap_or(14);
        let align = match b.align.as_deref() {
            Some("center") => "center",
            Some("right") => "right",
            Some("justify") => "justify",
            _ => "left",
        };
        let indent = b.indent.unwrap_or(0);
        let cells = block_cells(b);
        let n = cells.len().clamp(1, 4);
        let indent_css = if indent > 0 && n == 1 {
            format!("text-indent:{}cm;", indent as f32 * 1.25)
        } else {
            String::new()
        };
        let style = format!(
            "font-family:{};font-size:{size}pt;text-align:{align};{indent_css}",
            css_font(font)
        );
        if n == 1 {
            let inner = if cells[0].is_empty() {
                "&nbsp;"
            } else {
                cells[0].as_str()
            };
            html.push_str(&format!("<p style=\"{style}\">{inner}</p>"));
        } else {
            html.push_str(&format!(
                r#"<table class="zed-header" style="{style}width:100%;border:none;"><tr>"#
            ));
            let width = 100 / n;
            for (i, cell) in cells.iter().enumerate() {
                let ta = if n == 2 && i + 1 == n {
                    "text-align:right;"
                } else {
                    ""
                };
                let inner = if cell.is_empty() { "&nbsp;" } else { cell };
                html.push_str(&format!(
                    r#"<td style="border:none;width:{width}%;vertical-align:top;{ta}">{inner}</td>"#
                ));
            }
            html.push_str("</tr></table>");
        }
    }
    html.push_str("</div>");
    html
}

fn css_font(name: &str) -> String {
    format!("'{}', Times, serif", name.replace('\'', ""))
}

pub fn format_date(iso: &str, fmt: &str) -> String {
    let chrono_fmt = match fmt {
        "d.m.y" => "%d.%m.%y",
        "Y-m-d" => "%Y-%m-%d",
        "d.m.Yg" => "%d.%m.%Y г.",
        _ => "%d.%m.%Y",
    };
    let date = chrono::NaiveDate::parse_from_str(&iso[..iso.len().min(10)], "%Y-%m-%d")
        .or_else(|_| chrono::NaiveDate::parse_from_str(iso, "%d.%m.%Y"))
        .or_else(|_| chrono::NaiveDate::parse_from_str(iso, "%d.%m.%y"));
    match date {
        Ok(d) => d.format(chrono_fmt).to_string(),
        Err(_) => iso.to_string(),
    }
}

pub fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_and_paragraph() {
        let raw = r#"{
          "v":1,
          "blocks":[
            {"type":"header","left":"ООО Ромашка","right":"Исх. {{num}}","size":12},
            {"type":"paragraph","align":"center","html":"<b>ПРИКАЗ</b>","size":18,"indent":1},
            {"type":"block","cols":4,"html":["А","Б","В","Г"]}
          ]
        }"#;
        let html = to_html(raw);
        assert!(html.contains("ООО Ромашка"), "{html}");
        assert!(html.contains("{{num}}"), "{html}");
        assert!(html.contains("text-align:center"), "{html}");
        assert!(html.contains("ПРИКАЗ"), "{html}");
        assert!(html.contains("text-indent:1.25cm"), "{html}");
        assert!(html.contains(">Г<"), "{html}");
    }
}
