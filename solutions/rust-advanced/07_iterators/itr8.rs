pub fn numbers_until_bad(items: &[&str]) -> Vec<i32> {
    items.iter().map_while(|s| s.parse().ok()).collect()
}

pub fn body_after_marker<'a>(lines: &[&'a str]) -> Vec<&'a str> {
    lines.iter().copied().skip_while(|l| *l != "---").skip(1).collect()
}

pub fn hashtags(posts: &[&str]) -> Vec<String> {
    posts
        .iter()
        .flat_map(|p| p.split_whitespace())
        .filter_map(|w| w.strip_prefix('#'))
        .filter(|t| !t.is_empty())
        .map(str::to_lowercase)
        .collect()
}

pub fn valid_ages(raw: &[&str]) -> Vec<u8> {
    raw.iter().filter_map(|s| s.trim().parse::<u8>().ok()).filter(|&age| age <= 150).collect()
}

pub fn compact(xs: Vec<Option<u32>>) -> Vec<u32> {
    xs.into_iter().flatten().collect()
}

pub fn prefix_below(xs: &[u32], limit: u32) -> Vec<u32> {
    xs.iter().copied().take_while(|&x| x < limit).collect()
}

fn main() {
    println!("{:?}", compact(vec![Some(1), None]));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reading_stops_at_the_first_bad_item() {
        assert_eq!(numbers_until_bad(&["1", "2", "x", "4"]), vec![1, 2], "после x читать нельзя, даже если дальше цифры");
        assert!(numbers_until_bad(&["x", "1"]).is_empty());
    }

    #[test]
    fn body_starts_after_the_marker() {
        assert_eq!(body_after_marker(&["title", "meta", "---", "a", "---", "b"]), vec!["a", "---", "b"], "второй разделитель остаётся в тексте");
        assert!(body_after_marker(&["no", "marker"]).is_empty());
    }

    #[test]
    fn hashtags_across_posts() {
        let got = hashtags(&["Learning #Rust and #Iterators", "no tags", "# alone #X #rust"]);
        assert_eq!(got, vec!["rust", "iterators", "x", "rust"], "повторы остаются, решётка и регистр убраны");
    }

    #[test]
    fn ages_are_filtered_not_clamped() {
        assert_eq!(valid_ages(&["30", "abc", " 7 ", "151", "-3", "150", "0"]), vec![30, 7, 150, 0]);
    }

    #[test]
    fn compacting_options() {
        assert_eq!(compact(vec![Some(1), None, Some(3)]), vec![1, 3]);
        assert!(compact(vec![None, None]).is_empty());
    }

    #[test]
    fn prefix_stops_for_good() {
        assert_eq!(prefix_below(&[1, 2, 9, 3], 5), vec![1, 2], "тройка после девятки уже не берётся");
        assert!(prefix_below(&[], 5).is_empty());
    }
}
