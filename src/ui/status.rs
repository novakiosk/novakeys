#[derive(serde::Serialize)]
pub struct Status<'a> {
    pub current_language: &'a str,
    pub languages: Vec<&'a str>,
    pub shift_pressed: bool,
    pub composition_enabled: bool,
}
pub fn write_status_snapshot(status: &serde_json::Value) -> Result<(), String> {
    use std::io::Write;
    let dir = crate::ipc::runtime_dir().map_err(|e| e.to_string())?;
    let mut file = tempfile::NamedTempFile::new_in(&dir).map_err(|e| e.to_string())?;
    file.write_all(
        serde_json::to_string_pretty(status)
            .map_err(|e| e.to_string())?
            .as_bytes(),
    )
    .map_err(|e| e.to_string())?;
    file.persist(dir.join(crate::constants::file_names::STATUS_FILE))
        .map_err(|e| e.to_string())?;
    Ok(())
}
