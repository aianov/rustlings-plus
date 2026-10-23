use std::sync::Arc;

use auth_service::repository::BoxFuture;
use auth_service::service::ProfileError;
use auth_service::{Account, AuthParts, AuthService, Mailbox, ProfileDirectory, Registration};
use contracts::{ErrorBody, ErrorCode, SystemClock, Username};
use service_runtime::Secret;

struct Profiles {
    answer: Result<u64, ProfileError>,
}

impl ProfileDirectory for Profiles {
    fn create(&self, username: String) -> BoxFuture<'_, Result<Account, ProfileError>> {
        Box::pin(async move {
            let user_id = self.answer.clone()?;
            Ok(Account::new(user_id, &username))
        })
    }
}

fn name(raw: &str) -> Username {
    Username::try_from(raw).expect("ник")
}

fn secret(raw: &str) -> Secret<String> {
    Secret::new(raw.to_string())
}

fn registration(raw: &str) -> Registration {
    Registration {
        username: name(raw),
        email: None,
        password: secret("correct horse"),
    }
}

#[tokio::test]
async fn registration_goes_through_the_profile_port() {
    let parts = AuthParts::memory(Arc::new(SystemClock), Arc::new(Mailbox::default()))
        .expect("сервис в памяти");
    let auth = AuthService::new(parts);
    let ok = Profiles { answer: Ok(7) };
    let account = auth
        .register(&ok, &registration("alice"))
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
        .register(&taken, &registration("bob"))
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
        .register(&down, &registration("carol"))
        .await
        .expect_err("сосед лежит");
    assert_eq!(unavailable.code(), "unavailable");
    let unknown = auth
        .login(&name("dave"), &secret("correct horse"))
        .await
        .expect_err("нет учётной записи");
    assert_eq!(unknown.code(), "unauthorized");
    let issued = auth
        .login(&name("ALICE"), &secret("correct horse"))
        .await
        .expect("вход");
    assert_eq!(issued.account.user_id, 7);
    let principal = auth.validate(&issued.access).await.expect("токен принят");
    assert_eq!(principal.user_id, 7);
    auth.logout(&issued.access).await.expect("выход");
    assert_eq!(
        auth.validate(&issued.access).await.map(|p| p.user_id).ok(),
        None,
        "отозванный токен не проходит"
    );
}
