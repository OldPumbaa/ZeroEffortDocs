use sqlx::SqlitePool;

use crate::error::AppError;
use crate::model::{now, ModuleStatus};

pub struct ModuleInfo {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub available: bool,
}

pub const CATALOG: &[ModuleInfo] = &[
    ModuleInfo {
        id: "employees",
        name: "Сотрудники",
        description: "Кадровая база. Документы потом смогут ссылаться на человека, а не копировать ФИО руками. Подключается отдельно — если не нужна, её нет.",
        available: false,
    },
    ModuleInfo {
        id: "archive",
        name: "Архив",
        description: "Старые файлы компании: кладёте папку как есть, ищете по имени и содержимому. Не путать с живыми документами ZED.",
        available: false,
    },
];

pub fn info(id: &str) -> Result<&'static ModuleInfo, AppError> {
    CATALOG
        .iter()
        .find(|m| m.id == id)
        .ok_or(AppError::NotFound)
}

pub async fn list(pool: &SqlitePool) -> Result<Vec<ModuleStatus>, AppError> {
    let mut out = Vec::with_capacity(CATALOG.len());
    for spec in CATALOG {
        let row: Option<(i64, Option<String>)> =
            sqlx::query_as("SELECT enabled, enabled_at FROM modules WHERE id = ?")
                .bind(spec.id)
                .fetch_optional(pool)
                .await?;
        let (enabled, enabled_at) = row.unwrap_or((0, None));
        out.push(ModuleStatus {
            id: spec.id.to_string(),
            name: spec.name.to_string(),
            description: spec.description.to_string(),
            enabled: enabled != 0,
            available: spec.available,
            enabled_at,
        });
    }
    Ok(out)
}

pub async fn set_enabled(
    pool: &SqlitePool,
    id: &str,
    enabled: bool,
) -> Result<ModuleStatus, AppError> {
    let spec = info(id)?;
    let ts = if enabled { Some(now()) } else { None };
    sqlx::query("INSERT INTO modules (id, enabled, enabled_at) VALUES (?, ?, ?) ON CONFLICT(id) DO UPDATE SET enabled = excluded.enabled, enabled_at = excluded.enabled_at")
        .bind(spec.id)
        .bind(enabled as i64)
        .bind(&ts)
        .execute(pool)
        .await?;
    Ok(ModuleStatus {
        id: spec.id.to_string(),
        name: spec.name.to_string(),
        description: spec.description.to_string(),
        enabled,
        available: spec.available,
        enabled_at: ts,
    })
}
