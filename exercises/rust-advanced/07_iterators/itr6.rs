// Две функции обходят коллекции циклом `for`, и обе перестали собираться, как только тест попробовал
// пользоваться данными после вызова: `for x in vec` забирает вектор целиком. Вызывающему код нужны
// и результат, и сами данные. `summary` собирает id постов в строку и сообщает, сколько постов было,
// `tag_matrix` строит пары «пользователь:тег» по всем сочетаниям в порядке «сначала пользователи».
// TODO: Почини обе функции. Сигнатуру меняет только `summary` (подумай, кому должны принадлежать данные
// после вызова), а в `tag_matrix` чини сам цикл: тесты передают вектор тегов по значению.

pub struct Post {
    pub id: u32,
    pub tags: Vec<String>,
}

pub fn summary(posts: Vec<Post>) -> (String, usize) {
    let mut ids = Vec::new();
    for p in posts {
        ids.push(p.id.to_string());
    }
    (ids.join(","), posts.len())
}

pub fn tag_matrix(users: &[&str], tags: Vec<String>) -> Vec<String> {
    let mut out = Vec::new();
    for u in users {
        for t in tags {
            out.push(format!("{u}:{t}"));
        }
    }
    out
}

fn main() {
    let posts = vec![Post { id: 1, tags: vec![] }];
    println!("{:?}", summary(&posts));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_leaves_posts_with_the_caller() {
        let posts = vec![Post { id: 1, tags: vec!["a".into()] }, Post { id: 2, tags: vec![] }];
        let (ids, n) = summary(&posts);
        assert_eq!(ids, "1,2");
        assert_eq!(n, 2);
        assert_eq!(posts[0].tags, vec!["a"], "посты остались у вызывающего");
    }

    #[test]
    fn summary_of_nothing() {
        assert_eq!(summary(&[]), (String::new(), 0));
    }

    #[test]
    fn matrix_covers_every_pair_in_order() {
        let got = tag_matrix(&["ann", "bob"], vec!["x".to_string(), "y".to_string()]);
        assert_eq!(got, vec!["ann:x", "ann:y", "bob:x", "bob:y"], "теги должны перебираться заново для каждого пользователя");
    }

    #[test]
    fn matrix_with_no_users_or_no_tags() {
        assert!(tag_matrix(&[], vec!["x".into()]).is_empty());
        assert!(tag_matrix(&["ann"], vec![]).is_empty());
    }
}
