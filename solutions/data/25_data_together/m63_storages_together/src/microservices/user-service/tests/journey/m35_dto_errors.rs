use std::sync::Arc;

use grpc_kit::Caller;
use proto::social::v1::users_server::Users;
use proto::social::v1::{CreateProfileRequest, GetProfileRequest, Profile, UpdateProfileRequest};
use proto::{domain_code, from_status};
use service_runtime::Health;
use tonic::{Code, Request};
use user_service::bootstrap::build;
use user_service::dto::{new_profile, profile_change};
use user_service::repository::Stores;
use user_service::{Config, Patch, UsersController};

fn change(display_name: Option<&str>, bio: Option<&str>, clear: &[&str]) -> UpdateProfileRequest {
    UpdateProfileRequest {
        display_name: display_name.map(str::to_string),
        bio: bio.map(str::to_string),
        clear_fields: clear.iter().map(|field| field.to_string()).collect(),
    }
}

fn as_user<T>(id: u64, message: T) -> Request<T> {
    let mut request = Request::new(message);
    request.extensions_mut().insert(Caller(id));
    request
}

async fn controller(stores: Option<Stores>) -> UsersController {
    let config = Config {
        stores: Some(stores.unwrap_or_else(Stores::memory)),
        ..Config::default()
    };
    build(&config, &Health::default()).await.expect("сборка")
}

async fn register(users: &UsersController, name: &str) -> Profile {
    let ask = CreateProfileRequest {
        username: name.to_string(),
    };
    users
        .create_profile(Request::new(ask))
        .await
        .expect("регистрация")
        .into_inner()
}

async fn patch(
    users: &UsersController,
    id: u64,
    ask: UpdateProfileRequest,
) -> Result<Profile, tonic::Status> {
    users
        .update_profile(as_user(id, ask))
        .await
        .map(tonic::Response::into_inner)
}

#[test]
fn registration_is_normalized_then_checked() {
    let name = |raw: &str| {
        new_profile(&CreateProfileRequest {
            username: raw.to_string(),
        })
    };
    assert_eq!(name("  Alice_1 ").expect("ник").as_ref(), "alice_1");
    for (raw, code) in [
        ("", "blank"),
        ("ab", "too_short"),
        ("a b", "invalid_char"),
        ("алиса", "invalid_char"),
        ("a_very_long_nickname_x", "too_long"),
    ] {
        let refused = name(raw).expect_err(raw);
        assert_eq!(refused.pairs(), [("username", code)], "{raw:?}: {refused}");
    }
}

#[test]
fn a_patch_has_three_states_per_field() {
    let keep = profile_change(&change(None, None, &[])).expect("пустой PATCH");
    assert_eq!((keep.display_name, keep.bio), (Patch::Keep, Patch::Keep));
    let mixed =
        profile_change(&change(Some("  Алиса   Петрова "), None, &["bio"])).expect("имя и очистка");
    match mixed.display_name {
        Patch::Set(name) => assert_eq!(name.as_ref(), "Алиса Петрова", "пробелы схлопнуты"),
        other => panic!("ждали Set, а пришло {other:?}"),
    }
    assert_eq!(mixed.bio, Patch::Clear);
    let refused = profile_change(&change(
        Some(&"я".repeat(41)),
        Some("строка\u{7}"),
        &["display_name", "avatar"],
    ))
    .expect_err("ошибки");
    assert_eq!(
        refused.pairs(),
        [
            ("clear_fields[1]", "not_allowed"),
            ("display_name", "set_and_clear"),
            ("bio", "control_char"),
        ],
        "все ошибки PATCH разом, с путями"
    );
    let blank = profile_change(&change(Some("   "), Some(""), &[])).expect_err("пусто");
    assert_eq!(
        blank.pairs(),
        [("display_name", "blank"), ("bio", "blank")],
        "пустая строка не очищает: для этого есть clear_fields"
    );
}

#[tokio::test]
async fn update_profile_sets_clears_and_keeps() {
    let users = controller(None).await;
    let alice = register(&users, "alice").await;
    assert!(alice.display_name.is_empty() && alice.bio.is_empty());
    let both = patch(
        &users,
        alice.id,
        change(Some("Алиса"), Some("пишу на Rust\nи люблю чай"), &[]),
    )
    .await
    .expect("PATCH");
    assert_eq!(
        (both.display_name.as_str(), both.bio.as_str()),
        ("Алиса", "пишу на Rust\nи люблю чай")
    );
    let cleared = patch(&users, alice.id, change(None, None, &["bio"]))
        .await
        .expect("очистка");
    assert_eq!(
        (cleared.display_name.as_str(), cleared.bio.as_str()),
        ("Алиса", ""),
        "очистили bio, имя не тронули"
    );
    let same = patch(&users, alice.id, change(None, None, &[]))
        .await
        .expect("пусто");
    assert_eq!(same, cleared, "пустой PATCH ничего не меняет");
    let refused = patch(
        &users,
        alice.id,
        change(Some("Алиса Новая"), Some("\u{0}"), &[]),
    )
    .await
    .expect_err("bio кривое");
    assert_eq!(refused.code(), Code::InvalidArgument);
    assert_eq!(domain_code(&refused), Some("validation"));
    assert_eq!(
        from_status(&refused, "users").fields(),
        [("bio", "control_char")]
    );
    let after = users
        .get_profile(Request::new(GetProfileRequest { user_id: alice.id }))
        .await
        .expect("профиль")
        .into_inner();
    assert_eq!(
        after.display_name, "Алиса",
        "PATCH применяется целиком или никак: верное имя не записалось рядом с кривым bio"
    );
    let ghost = patch(&users, 99, change(Some("Призрак"), None, &[]))
        .await
        .expect_err("нет такого");
    assert_eq!(ghost.code(), Code::NotFound);
    let anonymous = users
        .update_profile(Request::new(change(Some("Кто-то"), None, &[])))
        .await
        .expect_err("без x-user-id");
    assert_eq!(anonymous.code(), Code::Unauthenticated);
}

#[tokio::test]
async fn profile_changes_survive_a_restart() {
    let stores = Stores::memory();
    let first = controller(Some(stores.clone())).await;
    let alice = register(&first, "alice").await;
    patch(&first, alice.id, change(Some("Алиса"), Some("о себе"), &[]))
        .await
        .expect("PATCH");
    patch(&first, alice.id, change(None, None, &["bio"]))
        .await
        .expect("очистка");
    drop(first);
    let second = controller(Some(stores)).await;
    let restored = second
        .get_profile(Request::new(GetProfileRequest { user_id: alice.id }))
        .await
        .expect("профиль после перезапуска")
        .into_inner();
    assert_eq!(
        (restored.display_name.as_str(), restored.bio.as_str()),
        ("Алиса", ""),
        "хранилище помнит и установку, и очистку"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn two_patches_at_once_do_not_lose_each_other() {
    let users = Arc::new(controller(None).await);
    for round in 0..20 {
        let name = format!("user{round}");
        let profile = register(&users, &name).await;
        let left = Arc::clone(&users);
        let right = Arc::clone(&users);
        let id = profile.id;
        let named =
            tokio::spawn(async move { patch(&left, id, change(Some("Имя"), None, &[])).await });
        let described =
            tokio::spawn(async move { patch(&right, id, change(None, Some("О себе"), &[])).await });
        named.await.expect("задача").expect("имя");
        described.await.expect("задача").expect("о себе");
        let after = users
            .get_profile(Request::new(GetProfileRequest { user_id: id }))
            .await
            .expect("профиль")
            .into_inner();
        assert_eq!(
            (after.display_name.as_str(), after.bio.as_str()),
            ("Имя", "О себе"),
            "PATCH применяется под замком репозитория: чтение и запись одной операцией, без потерянных обновлений"
        );
    }
}
