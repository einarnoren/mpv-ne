//! Minimal Win32 text-input prompt using TaskDialog + edit control.
//! Falls back to a simple InputBox-style dialog built with CreateWindowEx.
#![cfg(target_os = "windows")]


/// Show a simple input dialog and return the entered text, or None if cancelled.
/// Uses PowerShell's InputBox as the simplest cross-version approach.
#[allow(dead_code)]
pub fn prompt_text(title: &str, prompt: &str) -> Option<String> {
    // Use PowerShell to show an InputBox dialog - works on all Windows versions.
    let script = format!(
        r#"Add-Type -AssemblyName Microsoft.VisualBasic; [Microsoft.VisualBasic.Interaction]::InputBox('{prompt}', '{title}', '')"#,
        prompt = prompt.replace('\'', "''"),
        title  = title.replace('\'', "''"),
    );

    let output = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output()
        .ok()?;

    if !output.status.success() { return None; }

    let result = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if result.is_empty() { None } else { Some(result) }
}
