use std::time::Duration;

#[derive(Debug, Clone, PartialEq)]
pub struct Post {
    pub id: u64,
    pub author: String,
    pub text: String,
    pub likes: u32,
}

pub const LATENCY: Duration = Duration::from_millis(40);

pub fn read_page(user: u64) -> Vec<Post> {
    std::thread::sleep(LATENCY);
    (1..=5)
        .map(|id| Post {
            id,
            author: format!("author{user}"),
            text: if id == 2 { "цитата \"раз\"\nи строка с \\ чертой".to_string() } else { format!("пост {id} пользователя {user}") },
            likes: (id * 3) as u32,
        })
        .collect()
}
