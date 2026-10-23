use posts::Post;
use users::User;

pub fn karma(user: &User, posts: &[Post]) -> u32 {
    let own = posts.iter().filter(|p| p.author == user.id).count();
    u32::try_from(own).unwrap_or(u32::MAX).saturating_mul(10)
}

pub fn report(user: &User, posts: &[Post]) -> String {
    format!("{}: karma {}", user.name, karma(user, posts))
}
