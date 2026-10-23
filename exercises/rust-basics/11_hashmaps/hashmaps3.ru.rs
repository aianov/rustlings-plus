// Дан список счетов футбольных матчей (по одному на строку). Каждая строка
// имеет вид "<team_1_name>,<team_2_name>,<team_1_goals>,<team_2_goals>"
// Пример: "England,France,4,2" (Англия забила 4 гола, Франция 2).
//
// Нужно построить таблицу результатов с названием команды, общим
// числом забитых командой голов и общим числом голов,
// пропущенных командой.

use std::collections::HashMap;

// Структура для хранения данных о голах команды.
#[derive(Default)]
struct TeamScores {
    goals_scored: u8,
    goals_conceded: u8,
}

fn build_score_table(results: &str) -> HashMap<&str, TeamScores> {
    // Название команды это ключ, а связанная с ней структура это значение.
    let mut scores = HashMap::<&str, TeamScores>::new();

    for line in results.lines() {
        let mut split_iterator = line.split(',');
        // ПРИМЕЧАНИЕ: мы используем `unwrap`, потому что обработкой ошибок ещё не занимались.
        let team_1_name = split_iterator.next().unwrap();
        let team_2_name = split_iterator.next().unwrap();
        let team_1_score: u8 = split_iterator.next().unwrap().parse().unwrap();
        let team_2_score: u8 = split_iterator.next().unwrap().parse().unwrap();

        // TODO: Заполни таблицу результатов извлечёнными данными.
        // Помни: голы, забитые командой 1, это число голов,
        // пропущенных командой 2. Аналогично, голы, забитые командой 2, это
        // число голов, пропущенных командой 1.
    }

    scores
}

fn main() {
    // Здесь можно по желанию поэкспериментировать.
}

#[cfg(test)]
mod tests {
    use super::*;

    const RESULTS: &str = "England,France,4,2
France,Italy,3,1
Poland,Spain,2,0
Germany,England,2,1
England,Spain,1,0";

    #[test]
    fn build_scores() {
        let scores = build_score_table(RESULTS);

        assert!(["England", "France", "Germany", "Italy", "Poland", "Spain"]
            .into_iter()
            .all(|team_name| scores.contains_key(team_name)));
    }

    #[test]
    fn validate_team_score_1() {
        let scores = build_score_table(RESULTS);
        let team = scores.get("England").unwrap();
        assert_eq!(team.goals_scored, 6);
        assert_eq!(team.goals_conceded, 4);
    }

    #[test]
    fn validate_team_score_2() {
        let scores = build_score_table(RESULTS);
        let team = scores.get("Spain").unwrap();
        assert_eq!(team.goals_scored, 0);
        assert_eq!(team.goals_conceded, 3);
    }
}
