fn main() {
    let ann = users::User::new(1, "ann");
    let posts = vec![posts::Post::new(&ann, "hello")];
    println!("{}", app::report(&ann, &posts));
}
