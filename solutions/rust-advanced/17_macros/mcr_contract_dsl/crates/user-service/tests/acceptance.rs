use contracts::Route;
use user_service::{dispatch, UserMethod};

#[test]
fn the_user_service_describes_its_methods_with_the_shared_macro() {
    assert_eq!(UserMethod::ALL.len(), 3);
    assert_eq!(
        UserMethod::ROUTES[1],
        Route { name: "users.follow", request: "FollowRequest", response: "Empty" }
    );
    assert!(UserMethod::duplicates().is_empty());
}

#[test]
fn dispatch_knows_exactly_the_declared_names() {
    assert_eq!(dispatch("users.register"), Ok(UserMethod::Register));
    assert_eq!(dispatch("users.unfollow"), Ok(UserMethod::Unfollow));
    assert_eq!(dispatch("users.ban"), Err("неизвестный метод users.ban".to_string()));
}
