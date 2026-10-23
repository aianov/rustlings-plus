// Каталог товаров. Метод `render` готовит витрину и для каждого товара ждёт медленное оформление
// (50 мс). Пока витрина рисуется, другая задача хочет добавить новый товар, и ей приходится ждать,
// пока отрисуются все. Замок держится всё это время.
// TODO: Почини render, чтобы добавление не ждало отрисовки. Витрина показывает снимок товаров
// на момент начала render.
use std::time::Duration;
use tokio::sync::Mutex;

struct Catalog {
    items: Mutex<Vec<String>>,
}

async fn decorate(item: &str) -> String {
    tokio::time::sleep(Duration::from_millis(50)).await;
    format!("[{item}]")
}

impl Catalog {
    fn new(items: &[&str]) -> Self {
        Catalog { items: Mutex::new(items.iter().map(|s| s.to_string()).collect()) }
    }

    async fn add(&self, item: &str) {
        self.items.lock().await.push(item.to_string());
    }

    async fn len(&self) -> usize {
        self.items.lock().await.len()
    }
}

impl Catalog {
    async fn render(&self) -> String {
        let items = self.items.lock().await;
        let mut out = String::new();
        for item in items.iter() {
            out.push_str(&decorate(item).await);
        }
        out
    }
}

#[tokio::main]
async fn main() {
    let catalog = Catalog::new(&["чайник", "кружка"]);
    println!("{}", catalog.render().await);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tokio::time::Instant;

    #[tokio::test(start_paused = true)]
    async fn add_does_not_wait_for_render() {
        let catalog = Arc::new(Catalog::new(&["a", "b", "c", "d"]));
        let rendering = {
            let catalog = Arc::clone(&catalog);
            tokio::spawn(async move { catalog.render().await })
        };
        tokio::time::sleep(Duration::from_millis(1)).await;
        let started = Instant::now();
        catalog.add("e").await;
        assert!(
            started.elapsed() < Duration::from_millis(10),
            "добавление ждало {:?}: render держит замок на всё время оформления",
            started.elapsed()
        );
        assert_eq!(rendering.await.unwrap(), "[a][b][c][d]", "витрина показывает снимок на момент начала");
        assert_eq!(catalog.len().await, 5, "добавленный товар остался в каталоге");
    }

    #[tokio::test(start_paused = true)]
    async fn render_sees_current_items() {
        let catalog = Catalog::new(&["x"]);
        catalog.add("y").await;
        assert_eq!(catalog.render().await, "[x][y]", "то, что добавлено до render, в витрине есть");
    }
}
