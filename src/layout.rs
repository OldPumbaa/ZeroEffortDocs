use serde::Deserialize;

use crate::model::Field;

#[derive(Debug, Deserialize)]
struct Layout {
    v: u32,
    blocks: Vec<Block>,
}

#[derive(Debug, Deserialize)]
struct Block {
    #[serde(rename = "type")]
    kind: String,
    align: Option<String>,
    font: Option<String>,
    size: Option<u32>,
    html: Option<String>,
    left: Option<String>,
    right: Option<String>,
    #[serde(default)]
    indent: Option<u32>,
}

pub fn is_layout_json(body: &str) -> bool {
    let t = body.trim_start();
    t.starts_with('{') && t.contains("\"blocks\"")
}

pub fn is_html(body: &str) -> bool {
    let t = body.trim_start();
    t.starts_with('<')
}

/// Turns stored template body (JSON layout or plain text) into HTML with `{{key}}` still in it.
pub fn to_html(body: &str) -> String {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if let Ok(layout) = serde_json::from_str::<Layout>(trimmed) {
        if layout.v >= 1 {
            return render_blocks(&layout.blocks);
        }
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
            Some(serde_json::Value::String(s)) => html_escape(s),
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
        let indent_css = if indent > 0 && b.kind != "header" {
            format!("text-indent:{}cm;", indent as f32 * 1.25)
        } else {
            String::new()
        };
        let style = format!(
            "font-family:{};font-size:{size}pt;text-align:{align};{indent_css}",
            css_font(font)
        );
        match b.kind.as_str() {
            "header" => {
                html.push_str(&format!(
                    r#"<table class="zed-header" style="{style}width:100%;border:none;"><tr><td style="border:none;width:50%;vertical-align:top;">{left}</td><td style="border:none;width:50%;vertical-align:top;text-align:right;">{right}</td></tr></table>"#,
                    left = b.left.as_deref().unwrap_or(""),
                    right = b.right.as_deref().unwrap_or(""),
                ));
            }
            _ => {
                let inner = b.html.as_deref().unwrap_or("&nbsp;");
                html.push_str(&format!("<p style=\"{style}\">{inner}</p>"));
            }
        }
    }
    html.push_str("</div>");
    html
}

fn css_font(name: &str) -> String {
    format!("'{}', Times, serif", name.replace('\'', ""))
}

fn html_escape(s: &str) -> String {
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
            {"type":"paragraph","align":"center","html":"<b>ПРИКАЗ</b>","size":18,"indent":1}
          ]
        }"#;
        let html = to_html(raw);
        assert!(html.contains("zed-header"), "{html}");
        assert!(html.contains("ООО Ромашка"), "{html}");
        assert!(html.contains("{{num}}"), "{html}");
        assert!(html.contains("text-align:center"), "{html}");
        assert!(html.contains("ПРИКАЗ"), "{html}");
        assert!(html.contains("text-indent:1.25cm"), "{html}");
    }
}
