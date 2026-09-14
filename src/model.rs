use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::AppError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldType {
    Text,
    Textarea,
    Number,
    Date,
    Checkbox,
    Select,
}

impl FieldType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Textarea => "textarea",
            Self::Number => "number",
            Self::Date => "date",
            Self::Checkbox => "checkbox",
            Self::Select => "select",
        }
    }

    pub fn parse(s: &str) -> Result<Self, AppError> {
        match s {
            "text" => Ok(Self::Text),
            "textarea" => Ok(Self::Textarea),
            "number" => Ok(Self::Number),
            "date" => Ok(Self::Date),
            "checkbox" => Ok(Self::Checkbox),
            "select" => Ok(Self::Select),
            other => Err(AppError::bad(format!("неизвестный тип поля: {other}"))),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Field {
    pub id: String,
    pub key: String,
    pub label: String,
    #[serde(rename = "type")]
    pub field_type: FieldType,
    pub required: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FieldInput {
    pub id: Option<String>,
    pub key: String,
    pub label: String,
    #[serde(rename = "type")]
    pub field_type: FieldType,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub options: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TemplateSummary {
    pub id: String,
    pub name: String,
    pub description: String,
    pub field_count: i64,
    pub document_count: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TemplateDetail {
    pub id: String,
    pub name: String,
    pub description: String,
    pub fields: Vec<Field>,
    pub document_count: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpsertTemplate {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub fields: Vec<FieldInput>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DocumentSummary {
    pub id: String,
    pub template_id: String,
    pub template_name: String,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DocumentDetail {
    pub id: String,
    pub title: String,
    pub template: TemplateDetail,
    pub values: serde_json::Map<String, Value>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpsertDocument {
    pub template_id: String,
    pub title: String,
    #[serde(default)]
    pub values: serde_json::Map<String, Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PatchDocument {
    pub title: String,
    #[serde(default)]
    pub values: serde_json::Map<String, Value>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Instance {
    pub id: String,
    pub name: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PatchInstance {
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModuleStatus {
    pub id: String,
    pub name: String,
    pub description: String,
    pub enabled: bool,
    pub available: bool,
    pub enabled_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Stats {
    pub templates: i64,
    pub documents: i64,
    pub modules_enabled: i64,
}

pub fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

pub fn is_valid_key(key: &str) -> bool {
    let mut chars = key.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() => {}
        _ => return false,
    }
    key.len() <= 64
        && key
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

pub fn require_name(name: &str, what: &str) -> Result<String, AppError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(AppError::bad(format!("укажите {what}")));
    }
    if name.len() > 200 {
        return Err(AppError::bad(format!("{what} слишком длинное")));
    }
    Ok(name.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys() {
        assert!(is_valid_key("full_name"));
        assert!(is_valid_key("a"));
        assert!(is_valid_key("f1"));
        assert!(!is_valid_key(""));
        assert!(!is_valid_key("1abc"));
        assert!(!is_valid_key("FullName"));
        assert!(!is_valid_key("full-name"));
        assert!(!is_valid_key("фио"));
    }
}
