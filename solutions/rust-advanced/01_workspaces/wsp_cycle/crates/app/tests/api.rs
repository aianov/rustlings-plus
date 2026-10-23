use contracts::UserId;
use posts::Post;
use users::User;

#[test]
fn user_id_is_a_shared_contract() {
    let ann = User::new(7, "ann");
    assert_eq!(ann.id, UserId(7), "User::new(7, ..) должен класть UserId(7) из крейта contracts");
}

#[test]
fn post_is_created_from_an_id_not_from_a_user() {
    let post = Post::new(UserId(7), "hi");
    assert_eq!(post.author, UserId(7), "автор поста это UserId");
    assert_eq!(post.text, "hi");
}

#[test]
fn karma_is_ten_per_own_post() {
    let ann = User::new(1, "ann");
    let bob = User::new(2, "bob");
    let posts = vec![Post::new(ann.id, "a"), Post::new(bob.id, "b"), Post::new(ann.id, "c")];
    assert_eq!(app::karma(&ann, &posts), 20, "у ann два поста, карма 20");
    assert_eq!(app::karma(&bob, &posts), 10, "у bob один пост, карма 10");
    assert_eq!(app::karma(&User::new(3, "eve"), &posts), 0, "у eve постов нет");
}

#[test]
fn report_mentions_name_and_karma() {
    let ann = User::new(1, "ann");
    let posts = vec![Post::new(ann.id, "a")];
    assert_eq!(app::report(&ann, &posts), "ann: karma 10");
}
