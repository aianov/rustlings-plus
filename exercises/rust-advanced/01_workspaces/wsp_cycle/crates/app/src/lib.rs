pub fn report(user: &users::User, posts: &[posts::Post]) -> String {
    format!("{}: karma {}", user.name, users::karma(user, posts))
}
