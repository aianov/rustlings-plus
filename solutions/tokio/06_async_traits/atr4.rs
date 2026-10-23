use std::future::Future;
use std::pin::Pin;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;

struct MemCache {
    entries: HashMap<u32, String>,
}

struct SlowCache {
    delay: Duration,
    inner: MemCache,
}

trait Cache: Send + Sync {
    fn get(&self, key: u32) -> Pin<Box<dyn Future<Output = Option<String>> + Send + '_>>;
}

impl Cache for MemCache {
    fn get(&self, key: u32) -> Pin<Box<dyn Future<Output = Option<String>> + Send + '_>> {
        Box::pin(async move { self.entries.get(&key).cloned() })
    }
}

impl Cache for SlowCache {
    fn get(&self, key: u32) -> Pin<Box<dyn Future<Output = Option<String>> + Send + '_>> {
        Box::pin(async move {
            sleep(self.delay).await;
            self.inner.get(key).await
        })
    }
}

async fn lookup(layers: &[Arc<dyn Cache>], key: u32) -> Option<(usize, String)> {
    for (index, layer) in layers.iter().enumerate() {
        if let Some(value) = layer.get(key).await {
            return Some((index, value));
        }
    }
    None
}

#[tokio::main]
async fn main() {
    let layers: Vec<Arc<dyn Cache>> = vec![Arc::new(MemCache { entries: HashMap::new() })];
    println!("{:?}", lookup(&layers, 1).await);
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::Instant;

    fn mem(entries: &[(u32, &str)]) -> MemCache {
        MemCache { entries: entries.iter().map(|(key, value)| (*key, value.to_string())).collect() }
    }

    fn layers() -> Vec<Arc<dyn Cache>> {
        vec![
            Arc::new(mem(&[(1, "l1-one")])),
            Arc::new(SlowCache { delay: Duration::from_millis(50), inner: mem(&[(2, "l2-two"), (3, "l2-three")]) }),
            Arc::new(mem(&[(3, "l3-three"), (4, "l3-four")])),
        ]
    }

    #[tokio::test(start_paused = true)]
    async fn the_first_layer_with_the_key_wins() {
        let layers = layers();
        assert_eq!(lookup(&layers, 1).await, Some((0, "l1-one".to_string())), "ключ есть в самом быстром слое");
        assert_eq!(lookup(&layers, 3).await, Some((1, "l2-three".to_string())), "ключ 3 есть в двух слоях: нужен верхний из них");
        assert_eq!(lookup(&layers, 4).await, Some((2, "l3-four".to_string())), "ключ найден только в нижнем слое");
        assert_eq!(lookup(&layers, 5).await, None, "ключа нет нигде");
    }

    #[tokio::test(start_paused = true)]
    async fn lower_layers_are_not_asked_after_a_hit() {
        let layers = layers();
        let start = Instant::now();
        lookup(&layers, 1).await;
        assert_eq!(start.elapsed(), Duration::ZERO, "попадание в верхнем слое не должно трогать медленный слой");
        lookup(&layers, 2).await;
        assert_eq!(start.elapsed(), Duration::from_millis(50), "медленный слой спрашивается один раз");
    }
}
