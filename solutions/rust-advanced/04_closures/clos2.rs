// Касса считает общую сумму и подписывает каждую покупку именем покупателя и накопленным итогом.
// Склейкой имён и сумм занимается готовая `pair_up`: для каждой пары она зовёт твоё замыкание.
// Имена читаются из `stats.names`, а итог копится в `stats.total`: это разные поля одной структуры,
// и замыкание обязано занять только то, что ему нужно, иначе компилятор не даст читать имена.
// TODO: Реализуй `tally` через `pair_up`. Для пары (имя, сумма) прибавь сумму к `stats.total` и верни
// `имя: сумма (всего N)`, где N итог после этой покупки. Пар столько, сколько в более короткой стороне.

pub struct Stats {
    pub total: u32,
    pub names: Vec<String>,
}

pub fn pair_up(names: &[String], amounts: &[u32], mut label: impl FnMut(&str, u32) -> String) -> Vec<String> {
    names.iter().zip(amounts).map(|(name, &amount)| label(name, amount)).collect()
}

pub fn tally(stats: &mut Stats, amounts: &[u32]) -> Vec<String> {
    pair_up(&stats.names, amounts, |name, amount| {
        stats.total += amount;
        format!("{name}: {amount} (total {})", stats.total)
    })
}

fn main() {
    let mut s = Stats { total: 0, names: vec!["ann".into()] };
    println!("{:?}", tally(&mut s, &[5]));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stats(total: u32, names: &[&str]) -> Stats {
        Stats { total, names: names.iter().map(|n| n.to_string()).collect() }
    }

    #[test]
    fn running_total_is_included() {
        let mut s = stats(0, &["ann", "bob", "cat"]);
        assert_eq!(
            tally(&mut s, &[5, 10, 1]),
            vec!["ann: 5 (total 5)", "bob: 10 (total 15)", "cat: 1 (total 16)"],
            "в скобках итог после этой покупки"
        );
        assert_eq!(s.total, 16, "итог должен остаться в структуре");
    }

    #[test]
    fn starts_from_existing_total() {
        let mut s = stats(100, &["ann"]);
        assert_eq!(tally(&mut s, &[1]), vec!["ann: 1 (total 101)"]);
        assert_eq!(s.total, 101);
    }

    #[test]
    fn shorter_side_wins() {
        let mut s = stats(0, &["ann", "bob"]);
        assert_eq!(tally(&mut s, &[7]).len(), 1, "сумма одна: подписей тоже одна");
        assert_eq!(s.total, 7, "лишнее имя без суммы ничего не прибавляет");
        let mut t = stats(0, &["ann"]);
        assert_eq!(tally(&mut t, &[1, 2, 3]).len(), 1);
        assert_eq!(t.total, 1, "лишние суммы без имени не учитываются");
    }

    #[test]
    fn goes_through_pair_up() {
        let text = include_str!("clos2.rs");
        assert!(text.matches("pair_up(").count() >= 2, "склейку делает pair_up, а не отдельный цикл: передай ей замыкание");
    }

    #[test]
    fn names_stay_intact() {
        let mut s = stats(0, &["ann", "bob"]);
        let _ = tally(&mut s, &[1, 2]);
        assert_eq!(s.names, vec!["ann", "bob"], "имена читаются, но не меняются");
    }
}
