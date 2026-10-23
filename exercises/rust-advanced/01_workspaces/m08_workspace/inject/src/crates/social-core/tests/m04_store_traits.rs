use social_core::{
    DomainError, MemNetwork, MemPostStore, MemUserStore, Network, Post, PostId, PostStore, User,
    UserId, UserStore, Visibility,
};

#[derive(Default)]
struct ListUserStore {
    users: Vec<User>,
    last_id: u64,
}

impl UserStore for ListUserStore {
    fn next_id(&mut self) -> UserId {
        self.last_id += 10;
        UserId(self.last_id)
    }

    fn insert(&mut self, user: User) {
        self.users.push(user);
    }

    fn get(&self, id: UserId) -> Option<&User> {
        self.users.iter().find(|user| user.id() == id)
    }

    fn id_by_name(&self, name: &str) -> Option<UserId> {
        self.users.iter().find(|user| user.name() == name).map(User::id)
    }

    fn count(&self) -> usize {
        self.users.len()
    }
}

fn users_in<U: UserStore, P: PostStore>(net: &Network<U, P>) -> usize {
    net.user_count()
}

#[test]
fn network_works_over_any_user_store() {
    let mut net = Network::with_stores(ListUserStore::default(), MemPostStore::new());
    let alice = net.register("Alice").expect("alice регистрируется в чужом хранилище");
    let bob = net.register("bob").expect("bob регистрируется в чужом хранилище");
    assert_eq!(alice, UserId(10), "id выдаёт хранилище (next_id), а не Network");
    assert_eq!(bob, UserId(20), "id выдаёт хранилище (next_id), а не Network");
    assert_eq!(
        net.register("ALICE"),
        Err(DomainError::NameTaken(String::from("alice"))),
        "занятость ника Network узнаёт у хранилища"
    );
    assert_eq!(users_in(&net), 2, "Network<U, P> работает с любым U: UserStore");
    let found = net.find_user(" alice ").expect("find_user ищет через хранилище");
    assert_eq!(found.id(), alice, "find_user должен найти alice");
    assert_eq!(net.follow(bob, alice), Ok(true), "подписки не зависят от хранилища");
    assert_eq!(net.follow(bob, UserId(99)), Err(DomainError::UnknownUser(UserId(99))), "неизвестный id тоже проверяется через хранилище");
    let post = net
        .publish(alice, "пост из чужого хранилища", Visibility::Public)
        .expect("публикация должна пройти");
    assert_eq!(net.like(bob, post), Ok(true), "лайк проходит через PostStore::get_mut");
    assert_eq!(net.post(post).map(Post::likes), Some(1), "лайк сохранился в хранилище");
}

#[test]
fn mem_network_is_ready_to_use() {
    let mut net = MemNetwork::new();
    let alice = net.register("alice").expect("регистрация должна пройти");
    assert_eq!(net.user(alice).map(User::name), Some("alice"), "MemNetwork::new() собирает сеть на хранилищах в памяти");
    assert_eq!(users_in(&net), 1, "MemNetwork это Network<MemUserStore, MemPostStore>");
}

#[test]
fn user_store_default_methods() {
    let mut store = MemUserStore::new();
    let id = store.next_id();
    store.insert(User::new(id, String::from("alice")));
    assert!(store.contains(id), "contains по умолчанию работает через get");
    assert!(!store.contains(UserId(99)), "contains для чужого id: false");
    assert!(store.is_taken("alice"), "is_taken по умолчанию работает через id_by_name");
    assert!(!store.is_taken("bob"), "bob не сохранён");
    assert_eq!(store.find_by_name("alice").map(User::id), Some(id), "find_by_name по умолчанию: id_by_name, потом get");
    assert!(store.find_by_name("bob").is_none(), "find_by_name для чужого ника: None");
    assert_eq!(store.count(), 1, "в хранилище один пользователь");
}

#[test]
fn post_store_default_methods() {
    let mut store = MemPostStore::new();
    assert_eq!(store.count(), 0, "пустое хранилище");
    assert!(store.last_by(UserId(1)).is_none(), "в пустом хранилище нет постов автора");
    let first = store.next_id();
    store.insert(Post::new(first, UserId(1), String::from("раз"), Visibility::Public));
    let second = store.next_id();
    store.insert(Post::new(second, UserId(2), String::from("чужой"), Visibility::Public));
    let third = store.next_id();
    store.insert(Post::new(third, UserId(1), String::from("два"), Visibility::Public));
    assert_ne!(first, second, "next_id выдаёт новые id");
    assert_eq!(store.count(), 3, "count по умолчанию считает all()");
    assert!(store.contains(second), "contains по умолчанию работает через get");
    assert!(!store.contains(PostId(99)), "contains для чужого id: false");
    assert_eq!(store.all().len(), 3, "all() отдаёт все посты");
    assert_eq!(store.all()[0].id(), first, "all() хранит порядок публикации");
    assert_eq!(store.last_by(UserId(1)).map(Post::text), Some("два"), "last_by по умолчанию находит самый свежий пост автора");
    if let Some(post) = store.get_mut(first) {
        post.like(UserId(2));
    }
    assert_eq!(store.get(first).map(Post::likes), Some(1), "get_mut отдаёт тот самый пост, а не копию");
}

#[test]
fn user_card() {
    let mut user = User::new(UserId(1), String::from("alice"));
    assert_eq!(user.to_string(), "@alice", "карточка без био: @ник");
    user.set_bio(String::from("Пишу на Rust"));
    assert_eq!(user.to_string(), "@alice: Пишу на Rust", "карточка с био: @ник: био");
}

#[test]
fn post_card() {
    let mut post = Post::new(PostId(7), UserId(1), String::from("Привет, мир!"), Visibility::Public);
    assert_eq!(post.to_string(), "[7] Привет, мир! (лайков: 0)", "карточка поста: [id] текст (лайков: N)");
    post.like(UserId(2));
    post.like(UserId(3));
    assert_eq!(post.to_string(), "[7] Привет, мир! (лайков: 2)", "в карточке видно число лайков");
    let repost = Post::repost(PostId(8), UserId(2), PostId(7));
    assert_eq!(repost.to_string(), "[8] репост [7] (лайков: 0)", "карточка репоста: [id] репост [исходный id] (лайков: N)");
}

#[test]
fn cards_work_with_format() {
    let user = User::new(UserId(1), String::from("bob"));
    assert_eq!(format!("кто: {user}"), "кто: @bob", "Display подхватывается в format!");
}
