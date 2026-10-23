use super::account::Account;

pub fn points_for(cents: u32) -> u32 {
    cents / 100
}

pub fn welcome_points(account: &Account, cents: u32) -> String {
    format!("{} earns {} points", account.name, points_for(cents))
}
