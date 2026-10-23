pub fn format_cents(cents: u32) -> String {
    format!("{}.{:02}", cents / 100, cents % 100)
}
