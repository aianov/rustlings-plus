pub fn overdrawn(balances: &[i64]) -> Vec<usize> {
    balances
        .iter()
        .enumerate()
        .filter(|(_, b)| **b < 0)
        .map(|(i, _)| i)
        .collect()
}

pub fn is_balanced(entries: &[i64]) -> bool {
    ledger_core::total(entries) == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audit_flags_negative_balance() {
        assert_eq!(overdrawn(&[10, -5, 3, -1]), vec![1, 3]);
    }

    #[test]
    fn audit_checks_that_entries_cancel_out() {
        assert!(is_balanced(&[5, -5]));
        assert!(!is_balanced(&[5, -4]));
    }
}
