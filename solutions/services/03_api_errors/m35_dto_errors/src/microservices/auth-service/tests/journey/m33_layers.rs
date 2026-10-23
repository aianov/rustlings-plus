use std::sync::Arc;

use auth_service::repository::BoxFuture;
use auth_service::service::ProfileError;
use auth_service::{
    Account, AuthService, MemoryAccounts, MemoryTokens, ProfileDirectory, Registration,
};
use contracts::{ErrorBody, ErrorCode, Username};

struct Profiles {
    answer: Result<u64, ProfileError>,
}

impl ProfileDirectory for Profiles {
    fn create(&self, username: String) -> BoxFuture<'_, Result<Account, ProfileError>> {
        Box::pin(async move {
            let user_id = self.answer.clone()?;
            Ok(Account {
                user_id,
                username,
                email: None,
            })
        })
    }
}

fn name(raw: &str) -> Username {
    Username::try_from(raw).expect("ник")
}

fn named(raw: &str) -> Registration {
    Registration::named(name(raw))
}

#[tokio::test]
async fn registration_goes_through_the_profile_port() {
    let auth = AuthService::new(
        Arc::new(MemoryAccounts::new()),
        Arc::new(MemoryTokens::default()),
    );
    let ok = Profiles { answer: Ok(7) };
    let account = auth
        .register(&ok, &named("alice"))
        .await
        .expect("регистрация");
    assert_eq!(
        account.user_id, 7,
        "id выдаёт user-service, auth-service только помнит его"
    );
    let taken = Profiles {
        answer: Err(ProfileError::Refused(ErrorBody::new(
            ErrorCode::Conflict,
            "ник @bob уже занят",
        ))),
    };
    let refused = auth
        .register(&taken, &named("bob"))
        .await
        .expect_err("занят");
    assert_eq!(
        (refused.code(), refused.to_string().as_str()),
        (ErrorCode::Conflict, "ник @bob уже занят"),
        "доменный отказ соседа проходит со своим кодом и текстом"
    );
    let down = Profiles {
        answer: Err(ProfileError::Unavailable),
    };
    let unavailable = auth
        .register(&down, &named("carol"))
        .await
        .expect_err("сосед лежит");
    assert_eq!(unavailable.code(), "unavailable");
    let unknown = auth
        .login(&name("dave"))
        .await
        .expect_err("нет учётной записи");
    assert_eq!(unknown.code(), "unauthorized");
    let (token, user) = auth.login(&name("ALICE")).await.expect("вход");
    assert_eq!(user.user_id, 7);
    assert_eq!(auth.validate(&token).await, Some(user));
    assert!(auth.logout(&token).await);
    assert_eq!(
        auth.validate(&token).await,
        None,
        "отозванный токен не проходит"
    );
}
