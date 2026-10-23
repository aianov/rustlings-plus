// Ветка комментариев грузится из хранилища: у каждого комментария есть ответы, у ответов свои, и
// так дальше. `load_tree` обходит дерево рекурсивно, и сейчас два беспокойства. Во-первых, async
// fn не может вызывать сама себя в таком виде, компилятор отказывается собирать. Во-вторых, даже
// когда соберётся, ответы одного комментария грузятся по очереди, а пользователь ждёт, пока
// загрузится самая длинная ветка, а не всё дерево по одному запросу.
// TODO: Почини `load_tree`: все ответы одного комментария грузятся одновременно.
use std::collections::HashMap;
use std::time::Duration;
use tokio::time::sleep;

struct Store {
    tree: HashMap<u32, Vec<u32>>,
}

impl Store {
    async fn children(&self, id: u32) -> Vec<u32> {
        sleep(Duration::from_millis(10)).await;
        self.tree.get(&id).cloned().unwrap_or_default()
    }
}

#[derive(Debug, PartialEq)]
struct Comment {
    id: u32,
    children: Vec<Comment>,
}

async fn load_tree(store: &Store, id: u32) -> Comment {
    let mut children = Vec::new();
    for child in store.children(id).await {
        children.push(load_tree(store, child).await);
    }
    Comment { id, children }
}

#[tokio::main]
async fn main() {
    let store = Store { tree: HashMap::from([(1, vec![2, 3])]) };
    println!("{:?}", load_tree(&store, 1).await);
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::Instant;

    fn leaf(id: u32) -> Comment {
        Comment { id, children: vec![] }
    }

    fn store() -> Store {
        Store { tree: HashMap::from([(1, vec![2, 3, 4]), (2, vec![5, 6]), (4, vec![7])]) }
    }

    #[tokio::test(start_paused = true)]
    async fn the_whole_tree_is_loaded_in_order() {
        let tree = load_tree(&store(), 1).await;
        let expected = Comment {
            id: 1,
            children: vec![
                Comment { id: 2, children: vec![leaf(5), leaf(6)] },
                leaf(3),
                Comment { id: 4, children: vec![leaf(7)] },
            ],
        };
        assert_eq!(tree, expected, "дерево должно повторять хранилище, ответы идут в том же порядке");
    }

    #[tokio::test(start_paused = true)]
    async fn siblings_are_loaded_together() {
        let start = Instant::now();
        load_tree(&store(), 1).await;
        assert_eq!(
            start.elapsed(),
            Duration::from_millis(30),
            "три уровня запросов по 10 мс: ответы одного комментария должны грузиться одновременно, иначе выйдет 70 мс"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_deep_chain_is_fine_too() {
        let tree: HashMap<u32, Vec<u32>> = (1..300).map(|id| (id, vec![id + 1])).collect();
        let store = Store { tree };
        let mut node = load_tree(&store, 1).await;
        let mut depth = 1;
        while let Some(next) = node.children.pop() {
            node = next;
            depth += 1;
        }
        assert_eq!(depth, 300, "цепочка из 300 комментариев загружается целиком");
    }
}
