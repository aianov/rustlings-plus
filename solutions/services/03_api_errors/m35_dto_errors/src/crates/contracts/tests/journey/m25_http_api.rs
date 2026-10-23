use contracts::{Credentials, Login, TOKEN_LEN, UserId, UserView, decode, encode, is_token};

#[test]
fn credentials_and_login_on_the_wire() {
    let credentials: Credentials =
        decode(r#"{"username": "Alice"}"#).expect("тело входа разбирается");
    assert_eq!(
        credentials.username.as_ref(),
        "alice",
        "ник в теле нормализуется"
    );
    assert!(
        decode::<Credentials>(r#"{"username": "a b"}"#).is_err(),
        "кривой ник в теле это ошибка разбора"
    );
    let login = Login {
        token: "0123456789abcdef0123456789abcdef".to_string(),
        user: UserView {
            id: UserId(1),
            username: String::from("alice"),
            display_name: None,
            bio: None,
        },
    };
    let line = encode(&login).expect("ответ входа кодируется");
    assert_eq!(
        line,
        r#"{"token":"0123456789abcdef0123456789abcdef","user":{"id":"u_1","username":"alice"}}"#,
        "ответ входа: токен и пользователь"
    );
    assert_eq!(decode::<Login>(&line), Ok(login), "и обратно");
}

#[test]
fn token_shape_is_fixed() {
    assert_eq!(TOKEN_LEN, 32, "128 бит это 32 шестнадцатеричных знака");
    assert!(is_token("00ff00ff00ff00ff00ff00ff00ff00ff"));
    for bad in [
        "",
        "00ff",
        "00FF00FF00FF00FF00FF00FF00FF00FF",
        "00ff00ff00ff00ff00ff00ff00ff00fg",
        "00ff00ff00ff00ff00ff00ff00ff00ff00",
    ] {
        assert!(!is_token(bad), "{bad:?} не токен");
    }
}
