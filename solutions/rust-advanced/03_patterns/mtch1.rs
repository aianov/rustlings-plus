#[derive(Debug, PartialEq)]
pub struct Vote {
    pub poll: u32,
    pub option: usize,
}

#[derive(Debug, PartialEq)]
pub enum VoteError {
    NotAVote,
    MissingPoll,
    BadPoll(String),
    MissingOption,
    BadOption(String),
    TrailingJunk(String),
}

pub fn parse_vote(line: &str) -> Result<Vote, VoteError> {
    let mut words = line.split_whitespace();
    let Some("vote") = words.next() else {
        return Err(VoteError::NotAVote);
    };
    let Some(poll) = words.next() else {
        return Err(VoteError::MissingPoll);
    };
    let Ok(poll_id) = poll.parse::<u32>() else {
        return Err(VoteError::BadPoll(poll.to_string()));
    };
    let Some(option) = words.next() else {
        return Err(VoteError::MissingOption);
    };
    let Ok(option_index) = option.parse::<usize>() else {
        return Err(VoteError::BadOption(option.to_string()));
    };
    if let Some(extra) = words.next() {
        return Err(VoteError::TrailingJunk(extra.to_string()));
    }
    Ok(Vote { poll: poll_id, option: option_index })
}

fn main() {
    println!("{:?}", parse_vote("vote 7 2"));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bad(line: &str) -> VoteError {
        parse_vote(line).expect_err("такая строка не должна разбираться")
    }

    #[test]
    fn good_lines() {
        assert_eq!(parse_vote("vote 7 2"), Ok(Vote { poll: 7, option: 2 }));
        assert_eq!(parse_vote("  vote   12\t0  "), Ok(Vote { poll: 12, option: 0 }), "лишние пробелы вокруг слов терпим");
    }

    #[test]
    fn not_a_vote() {
        assert_eq!(bad(""), VoteError::NotAVote, "пустая строка не команда");
        assert_eq!(bad("poll 7 2"), VoteError::NotAVote, "чужая команда");
        assert_eq!(bad("VOTE 7 2"), VoteError::NotAVote, "имя команды чувствительно к регистру");
        assert_eq!(bad("7 2"), VoteError::NotAVote, "число на месте команды");
    }

    #[test]
    fn broken_poll() {
        assert_eq!(bad("vote"), VoteError::MissingPoll, "после vote ничего нет");
        assert_eq!(bad("vote x 2"), VoteError::BadPoll("x".into()), "номер опроса не число");
        assert_eq!(bad("vote -1 2"), VoteError::BadPoll("-1".into()), "минус для u32 это не число");
        assert_eq!(bad("vote 99999999999 2"), VoteError::BadPoll("99999999999".into()), "число не влезает в u32");
    }

    #[test]
    fn broken_option() {
        assert_eq!(bad("vote 7"), VoteError::MissingOption, "варианта нет");
        assert_eq!(bad("vote 7 two"), VoteError::BadOption("two".into()), "вариант не число");
    }

    #[test]
    fn earliest_problem_wins() {
        assert_eq!(bad("vote x"), VoteError::BadPoll("x".into()), "сначала ругаемся на опрос, а не на пропавший вариант");
        assert_eq!(bad("vote x y z"), VoteError::BadPoll("x".into()), "из трёх проблем называется первая");
        assert_eq!(bad("vote 7 y z"), VoteError::BadOption("y".into()), "лишнее слово хуже плохого варианта не считается");
    }

    #[test]
    fn trailing_words_are_reported() {
        assert_eq!(bad("vote 7 2 3"), VoteError::TrailingJunk("3".into()));
        assert_eq!(bad("vote 7 2 now please"), VoteError::TrailingJunk("now".into()), "называем первое лишнее слово");
    }

    #[test]
    fn happy_path_is_flat() {
        let prod = include_str!("mtch1.rs").split("#[cfg(test)]").next().expect("файл не пуст");
        let flat = prod
            .lines()
            .filter(|l| l.trim_start().starts_with("let ") && l.trim_end().ends_with("else {"))
            .count();
        assert!(flat >= 3, "в этой задаче цель именно `let ... else`: найдено {flat} таких строк, нужно хотя бы три");
    }
}
