use contracts::Route;
use post_service::{dispatch, PostMethod};

#[test]
fn the_post_service_uses_the_same_macro_for_its_own_table() {
    assert_eq!(PostMethod::ALL.len(), 2);
    let list = PostMethod::ROUTES[1];
    assert_eq!(list.name, "posts.list");
    assert_eq!(list.request, "ListRequest");
    assert_eq!(list.response.replace(' ', ""), "Page<Post>");
    assert_eq!(PostMethod::ROUTES[0], Route { name: "posts.create", request: "CreateRequest", response: "Post" });
}

#[test]
fn dispatch_rejects_foreign_names() {
    assert_eq!(dispatch("posts.create"), Ok(PostMethod::Create));
    assert_eq!(dispatch("users.register"), Err("неизвестный метод users.register".to_string()));
}
