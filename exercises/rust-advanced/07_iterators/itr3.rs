// Две функции что-то «делают» с постами через цепочку адаптеров: первая должна запомнить id просмотренных,
// вторая записать в журнал строку о каждом популярном посте и вернуть их число. Тесты видят пустой
// список и пустой журнал, хотя функции отрабатывают без ошибок. Компилятор предупреждает о ненужном
// результате, но предупреждение легко пропустить.
// TODO: Найди, почему ничего не происходит, и почини обе функции. Результат второй должен быть числом
// популярных (лайков не меньше десяти).

pub struct Post {
    pub id: u32,
    pub likes: u32,
}

pub fn mark_seen(posts: &[Post], seen: &mut Vec<u32>) {
    posts.iter().map(|p| seen.push(p.id));
}

pub fn count_popular(posts: &[Post], log: &mut Vec<String>) -> usize {
    posts.iter().filter(|p| p.likes >= 10).inspect(|p| log.push(format!("popular {}", p.id)));
    0
}

fn main() {
    let mut seen = Vec::new();
    mark_seen(&[Post { id: 1, likes: 0 }], &mut seen);
    println!("{seen:?}");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn posts() -> Vec<Post> {
        vec![Post { id: 1, likes: 3 }, Post { id: 2, likes: 12 }, Post { id: 3, likes: 10 }]
    }

    #[test]
    fn seen_ids_are_recorded_in_order() {
        let mut seen = vec![0];
        mark_seen(&posts(), &mut seen);
        assert_eq!(seen, vec![0, 1, 2, 3], "id просмотренных дописываются после уже записанных");
    }

    #[test]
    fn popular_posts_are_logged_and_counted() {
        let mut log = Vec::new();
        let n = count_popular(&posts(), &mut log);
        assert_eq!(n, 2, "популярные: лайков не меньше десяти");
        assert_eq!(log, vec!["popular 2", "popular 3"], "запись в журнал по одному разу на пост, в порядке постов");
    }

    #[test]
    fn nothing_popular_is_fine() {
        let mut log = Vec::new();
        assert_eq!(count_popular(&[Post { id: 1, likes: 0 }], &mut log), 0);
        assert!(log.is_empty());
    }
}
