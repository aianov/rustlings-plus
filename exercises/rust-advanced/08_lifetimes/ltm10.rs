// Арена: бойцы лежат в срезе, а раунд это «боец `i` бьёт бойца `j`». Функция `duel(attacker, defender)`
// принимает две изменяемые ссылки сразу, и вот тут начинается беда: `&mut fighters[i]` и `&mut fighters[j]`
// вместе компилятор не пропускает, он не знает, что `i` и `j` разные. Нужно честно разделить срез на
// непересекающиеся части. Порядок аргументов важен (бьёт именно `i`), а `i == j` это ошибка.
// Вторая функция: чемпион (первый в срезе) по очереди бьёт всех остальных.
// TODO: Допиши `arena_round` и `champion_vs_rest` через `split_at_mut` и `split_first_mut`.

#[derive(Debug, PartialEq)]
pub struct Fighter {
    pub name: &'static str,
    pub hp: i32,
    pub power: i32,
}

pub fn duel(attacker: &mut Fighter, defender: &mut Fighter) {
    defender.hp -= attacker.power;
    if defender.hp > 0 {
        attacker.hp -= defender.power;
    }
}

pub fn arena_round(fighters: &mut [Fighter], i: usize, j: usize) -> Result<(), String> {
    todo!("найти двух разных бойцов и вызвать duel")
}

pub fn champion_vs_rest(fighters: &mut [Fighter]) {
    todo!("первый бьёт каждого из остальных по очереди")
}

fn main() {
    let mut f = vec![Fighter { name: "a", hp: 10, power: 3 }, Fighter { name: "b", hp: 10, power: 2 }];
    println!("{:?}", arena_round(&mut f, 0, 1));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn team() -> Vec<Fighter> {
        vec![
            Fighter { name: "ann", hp: 20, power: 5 },
            Fighter { name: "bob", hp: 12, power: 4 },
            Fighter { name: "cat", hp: 7, power: 3 },
        ]
    }

    #[test]
    fn lower_index_attacks_higher() {
        let mut f = team();
        arena_round(&mut f, 0, 2).unwrap();
        assert_eq!((f[0].hp, f[2].hp), (20 - 3, 7 - 5), "ann бьёт cat (7 - 5 = 2), cat остаётся стоять и отвечает на 3");
    }

    #[test]
    fn higher_index_can_attack_too() {
        let mut f = team();
        arena_round(&mut f, 2, 0).unwrap();
        assert_eq!((f[2].hp, f[0].hp), (7 - 5, 20 - 3), "бьёт именно cat: у ann стало 17, у cat после ответа 2");
    }

    #[test]
    fn a_defeated_defender_does_not_strike_back() {
        let mut f = team();
        f[2].hp = 5;
        arena_round(&mut f, 0, 2).unwrap();
        assert_eq!((f[0].hp, f[2].hp), (20, 0));
    }

    #[test]
    fn bad_indexes_are_errors_and_change_nothing() {
        let mut f = team();
        assert!(arena_round(&mut f, 1, 1).is_err(), "боец не бьёт сам себя");
        assert!(arena_round(&mut f, 0, 3).is_err());
        assert!(arena_round(&mut f, 9, 0).is_err());
        assert_eq!(f, team(), "после ошибки состояние не меняется");
    }

    #[test]
    fn middle_fighters_work_too() {
        let mut f = team();
        arena_round(&mut f, 1, 0).unwrap();
        assert_eq!((f[1].hp, f[0].hp), (12 - 5, 20 - 4), "bob бьёт ann, ann отвечает");
    }

    #[test]
    fn champion_fights_everyone_in_order() {
        let mut f = team();
        champion_vs_rest(&mut f);
        assert_eq!((f[0].hp, f[1].hp, f[2].hp), (20 - 4 - 3, 12 - 5, 7 - 5));
        let mut single = vec![Fighter { name: "solo", hp: 1, power: 1 }];
        champion_vs_rest(&mut single);
        assert_eq!(single[0].hp, 1);
        let mut none: Vec<Fighter> = Vec::new();
        champion_vs_rest(&mut none);
    }
}
