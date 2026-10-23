pub fn table(rows: &[(&str, u32)]) -> String {
    let width = rows.iter().map(|(name, _)| name.len()).max().unwrap_or(0);
    rows.iter()
        .map(|(name, value)| format!("{name:<width$} | {value}"))
        .collect::<Vec<_>>()
        .join("\n")
}
