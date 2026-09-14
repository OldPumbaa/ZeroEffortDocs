use std::path::Path;

use crate::error::AppError;

pub fn print_docx(path: &Path) -> Result<(), AppError> {
    #[cfg(windows)]
    {
        print_with_word(path)
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        Err(AppError::bad(
            "отправка в Word есть только на Windows. Распечатайте предпросмотр",
        ))
    }
}

#[cfg(windows)]
fn print_with_word(path: &Path) -> Result<(), AppError> {
    let abs = path
        .canonicalize()
        .map_err(|e| AppError::bad(format!("нет файла для печати: {e}")))?;
    let path_str = abs.to_string_lossy().replace('\'', "''");
    let script = format!(
        r#"
$ErrorActionPreference = 'Stop'
$word = $null
$doc = $null
try {{
  $word = New-Object -ComObject Word.Application
  $word.Visible = $false
  $word.DisplayAlerts = 0
  $doc = $word.Documents.Open('{path_str}', $false, $true)
  $doc.PrintOut($false)
  $n = 0
  while ($word.BackgroundPrintingStatus -gt 0 -and $n -lt 80) {{
    Start-Sleep -Milliseconds 250
    $n++
  }}
}} catch {{
  throw
}} finally {{
  if ($doc -ne $null) {{ $doc.Close($false) | Out-Null }}
  if ($word -ne $null) {{ $word.Quit() | Out-Null }}
}}
"#
    );
    let out = std::process::Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-STA",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            &script,
        ])
        .output()
        .map_err(|e| AppError::bad(format!("не удалось запустить печать: {e}")))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let err = err.trim();
        if err.contains("80040154") || err.to_ascii_lowercase().contains("com") {
            return Err(AppError::bad(
                "Word не установлен. Скачайте файл и печатайте из Word, либо распечатайте предпросмотр",
            ));
        }
        return Err(AppError::bad(if err.is_empty() {
            "Word не принял печать".into()
        } else {
            format!("Word: {err}")
        }));
    }
    Ok(())
}
