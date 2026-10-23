use contracts::UserId;

fn main() {
    let ann = users::User::new(1, "ann");
    let posts = vec![posts::Post::new(UserId(1), "hello")];
    println!("{}", app::report(&ann, &posts));
}
