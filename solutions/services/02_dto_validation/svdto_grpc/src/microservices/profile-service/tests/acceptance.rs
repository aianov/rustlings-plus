use profile_service::bootstrap::build_with;
use profile_service::domain::Profile;
use profile_service::pb::profiles_server::Profiles;
use profile_service::pb::{GetProfileRequest, UpdateProfileRequest};
use tonic::{Code, Request};

fn seed() -> Vec<(u64, Profile)> {
    vec![(
        1,
        Profile { display_name: "Alice".to_string(), bio: Some("люблю Rust".to_string()), birth_year: Some(1999) },
    )]
}

fn update(clear: &[&str]) -> UpdateProfileRequest {
    UpdateProfileRequest {
        user_id: 1,
        display_name: None,
        bio: None,
        birth_year: None,
        clear_fields: clear.iter().map(|s| s.to_string()).collect(),
    }
}

#[tokio::test]
async fn only_sent_fields_change() {
    let controller = build_with(seed());
    let request = UpdateProfileRequest { bio: Some("новое био".to_string()), ..update(&[]) };
    let reply = controller.update_profile(Request::new(request)).await.unwrap().into_inner();
    assert_eq!(reply.bio, "новое био", "bio прислано и должно измениться");
    assert_eq!(reply.display_name, "Alice", "displayName не прислано: остаётся прежним");
    assert_eq!(reply.birth_year, 1999, "birthYear не прислано: остаётся прежним");
}

#[tokio::test]
async fn empty_string_is_a_value_not_absence() {
    let controller = build_with(seed());
    let request = UpdateProfileRequest { display_name: Some(String::new()), ..update(&[]) };
    let status = controller.update_profile(Request::new(request)).await.unwrap_err();
    assert_eq!(status.code(), Code::InvalidArgument, "присланная пустая строка это значение, и оно не проходит проверку");
    assert_eq!(status.message(), "displayName: too_short", "сообщение называет поле и код");
}

#[tokio::test]
async fn zero_year_is_a_value_not_absence() {
    let controller = build_with(seed());
    let request = UpdateProfileRequest { birth_year: Some(0), ..update(&[]) };
    let status = controller.update_profile(Request::new(request)).await.unwrap_err();
    assert_eq!(status.message(), "birthYear: out_of_range", "0 присланный явно это год 0, а не «не прислано»");
}

#[tokio::test]
async fn clear_fields_means_null() {
    let controller = build_with(seed());
    let reply = controller.update_profile(Request::new(update(&["bio", "birthYear"]))).await.unwrap().into_inner();
    assert_eq!((reply.bio.as_str(), reply.birth_year), ("", 0), "bio и birthYear очищены (в ответе пустые значения)");
    assert_eq!(reply.display_name, "Alice", "имя на месте");
    let stored = controller.get_profile(Request::new(GetProfileRequest { user_id: 1 })).await.unwrap().into_inner();
    assert_eq!(stored.bio, "", "очистка сохранилась");
}

#[tokio::test]
async fn display_name_cannot_be_cleared() {
    let controller = build_with(seed());
    let status = controller.update_profile(Request::new(update(&["displayName"]))).await.unwrap_err();
    assert_eq!(status.code(), Code::InvalidArgument, "имя очищать нельзя");
    assert_eq!(status.message(), "displayName: null_not_allowed", "сообщение");
}

#[tokio::test]
async fn contradictory_and_unknown_clears_are_rejected() {
    let controller = build_with(seed());
    let request = UpdateProfileRequest { bio: Some("x".to_string()), ..update(&["bio"]) };
    let status = controller.update_profile(Request::new(request)).await.unwrap_err();
    assert_eq!(status.message(), "bio: conflict", "и значение, и очистка для одного поля");
    let status = controller.update_profile(Request::new(update(&["role"]))).await.unwrap_err();
    assert_eq!(status.message(), "role: unknown_field", "чистить можно только известные поля");
}

#[tokio::test]
async fn all_problems_are_listed_in_one_status() {
    let controller = build_with(seed());
    let request = UpdateProfileRequest {
        display_name: Some("  ".to_string()),
        bio: Some("б".repeat(161)),
        birth_year: Some(1800),
        ..update(&[])
    };
    let status = controller.update_profile(Request::new(request)).await.unwrap_err();
    assert_eq!(
        status.message(),
        "displayName: too_short; bio: too_long; birthYear: out_of_range",
        "все ошибки через «; » в порядке displayName, bio, birthYear"
    );
    let stored = controller.get_profile(Request::new(GetProfileRequest { user_id: 1 })).await.unwrap().into_inner();
    assert_eq!(stored.display_name, "Alice", "при ошибках профиль не меняется");
}

#[tokio::test]
async fn unknown_user_is_not_found() {
    let controller = build_with(seed());
    let status = controller
        .update_profile(Request::new(UpdateProfileRequest { user_id: 99, bio: Some("x".to_string()), ..update(&[]) }))
        .await
        .unwrap_err();
    assert_eq!(status.code(), Code::NotFound, "нет такого пользователя");
    let status = controller.get_profile(Request::new(GetProfileRequest { user_id: 99 })).await.unwrap_err();
    assert_eq!(status.code(), Code::NotFound, "и при чтении тоже");
}
