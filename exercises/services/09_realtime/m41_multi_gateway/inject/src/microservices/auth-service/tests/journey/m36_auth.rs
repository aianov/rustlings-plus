use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use auth_service::service::Verdict;
use auth_service::service::token::{self, Claims, KeyRing, TokenPolicy, VerifyError, base64, hmac};
use auth_service::{Cost, Passwords};
use service_runtime::Secret;

fn hex(bytes: &[u8]) -> String {
    hmac::hex(bytes)
}

#[test]
fn base64url_follows_rfc_4648_and_is_strict() {
    for (raw, encoded) in [
        ("", ""),
        ("f", "Zg"),
        ("fo", "Zm8"),
        ("foo", "Zm9v"),
        ("foob", "Zm9vYg"),
        ("fooba", "Zm9vYmE"),
        ("foobar", "Zm9vYmFy"),
    ] {
        assert_eq!(base64::encode(raw.as_bytes()), encoded, "{raw:?}");
        assert_eq!(base64::decode(encoded).as_deref(), Some(raw.as_bytes()));
    }
    assert_eq!(
        base64::encode(&[0xfb, 0xff]),
        "-_8",
        "алфавит url: - и _ вместо + и /"
    );
    for bad in ["Zh", "Zm9", "Zm9v=", "Zm 9v", "Z", "Zm9v+A"] {
        assert_eq!(
            base64::decode(bad),
            None,
            "{bad:?}: у одной строки байтов одна запись, иначе подписи и списки отзыва сравнивают разное"
        );
    }
}

#[test]
fn hmac_matches_rfc_4231() {
    let case1 = hmac::hmac_sha256(&[0x0b; 20], b"Hi There");
    assert_eq!(
        hex(&case1),
        "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7"
    );
    let case2 = hmac::hmac_sha256(b"Jefe", b"what do ya want for nothing?");
    assert_eq!(
        hex(&case2),
        "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
    );
    let case6 = hmac::hmac_sha256(
        &[0xaa; 131],
        b"Test Using Larger Than Block-Size Key - Hash Key First",
    );
    assert_eq!(
        hex(&case6),
        "60e431591ee0b67f0d8a26aacbf5b77f8e0bc6213728c5140546040f0ee37f54",
        "ключ длиннее блока сначала хешируется"
    );
    assert!(
        hmac::same(b"abc", b"abc") && !hmac::same(b"abc", b"abd") && !hmac::same(b"ab", b"abc")
    );
}

fn claims(now: u64) -> Claims {
    Claims {
        sub: 7,
        name: "alice".to_string(),
        role: "user".to_string(),
        iss: token::ISSUER.to_string(),
        aud: token::AUDIENCE.to_string(),
        iat: now,
        nbf: now,
        exp: now + 900,
        jti: "jti-1".to_string(),
        sid: "s-1".to_string(),
    }
}

fn keys() -> KeyRing {
    KeyRing::new("k1", vec![7; 32])
}

fn forge(header: &str, body: &str, signature: &str) -> String {
    format!(
        "{}.{}.{}",
        base64::encode(header.as_bytes()),
        base64::encode(body.as_bytes()),
        signature
    )
}

#[test]
fn signature_first_then_time_issuer_and_audience() {
    let policy = TokenPolicy::default();
    let now = 1_000_000;
    let token = token::sign(&keys(), &claims(now));
    assert_eq!(token.split('.').count(), 3);
    assert_eq!(
        token::verify(&keys(), &token, &policy, now),
        Ok(claims(now))
    );
    let body = serde_json::to_string(&claims(now)).expect("claims");
    let none = forge(r#"{"alg":"none","kid":"k1"}"#, &body, "");
    assert_eq!(
        token::verify(&keys(), &none, &policy, now),
        Err(VerifyError::UnsupportedAlg),
        "алгоритм не выбирает клиент: none это отказ"
    );
    let stranger = forge(r#"{"alg":"HS256","kid":"k9"}"#, &body, "AAAA");
    assert_eq!(
        token::verify(&keys(), &stranger, &policy, now),
        Err(VerifyError::UnknownKey)
    );
    let mut late = claims(now);
    late.exp = now - 10_000;
    let forged = forge(
        r#"{"alg":"HS256","kid":"k1"}"#,
        &serde_json::to_string(&late).expect("claims"),
        &base64::encode(&[0; 32]),
    );
    assert_eq!(
        token::verify(&keys(), &forged, &policy, now),
        Err(VerifyError::BadSignature),
        "срок подделки не читается, пока не сошлась подпись"
    );
    let tampered = {
        let mut parts: Vec<&str> = token.split('.').collect();
        let admin = serde_json::to_string(&Claims {
            role: "admin".to_string(),
            ..claims(now)
        })
        .expect("claims");
        let encoded = base64::encode(admin.as_bytes());
        parts[1] = &encoded;
        parts.join(".")
    };
    assert_eq!(
        token::verify(&keys(), &tampered, &policy, now),
        Err(VerifyError::BadSignature)
    );
    assert_eq!(
        token::verify(&keys(), "a.b", &policy, now),
        Err(VerifyError::Malformed)
    );
}

#[test]
fn time_has_a_leeway_and_exact_edges() {
    let policy = TokenPolicy::default();
    let now = 1_000_000;
    let token = token::sign(&keys(), &claims(now));
    let edge = now + 900 + policy.leeway;
    assert!(token::verify(&keys(), &token, &policy, edge - 1).is_ok());
    assert_eq!(
        token::verify(&keys(), &token, &policy, edge),
        Err(VerifyError::Expired),
        "now >= exp + leeway: в секунду окончания уже нельзя"
    );
    let early = Claims {
        nbf: now + 100,
        ..claims(now)
    };
    let early = token::sign(&keys(), &early);
    assert_eq!(
        token::verify(&keys(), &early, &policy, now + 100 - policy.leeway - 1),
        Err(VerifyError::NotYetValid)
    );
    assert!(token::verify(&keys(), &early, &policy, now + 100 - policy.leeway).is_ok());
    let foreign = token::sign(
        &keys(),
        &Claims {
            aud: "payments".to_string(),
            ..claims(now)
        },
    );
    assert_eq!(
        token::verify(&keys(), &foreign, &policy, now),
        Err(VerifyError::WrongAudience),
        "токен платёжного сервиса соцсеть не открывает"
    );
    let other = token::sign(
        &keys(),
        &Claims {
            iss: "someone-else".to_string(),
            ..claims(now)
        },
    );
    assert_eq!(
        token::verify(&keys(), &other, &policy, now),
        Err(VerifyError::WrongIssuer)
    );
}

#[test]
fn rotation_keeps_old_tokens_until_the_old_key_retires() {
    let policy = TokenPolicy::default();
    let now = 1_000_000;
    let mut ring = keys();
    let old = token::sign(&ring, &claims(now));
    ring.rotate("k2", vec![9; 32]);
    let fresh = token::sign(&ring, &claims(now));
    assert!(fresh.starts_with(&base64::encode(br#"{"alg":"HS256","kid":"k2"}"#)));
    assert!(
        token::verify(&ring, &old, &policy, now).is_ok(),
        "смена ключа никого не разлогинила"
    );
    assert!(token::verify(&ring, &fresh, &policy, now).is_ok());
    assert!(ring.retire("k1"));
    assert_eq!(
        token::verify(&ring, &old, &policy, now),
        Err(VerifyError::UnknownKey)
    );
    let parsed = KeyRing::parse(&format!(
        "k3:{},k2:{}",
        base64::encode(&[1; 32]),
        base64::encode(&[2; 32])
    ))
    .expect("связка из конфига");
    assert_eq!(parsed.current().kid, "k3");
    assert!(parsed.find("k2").is_some());
    assert!(
        KeyRing::parse("k1:c2hvcnQ").is_err(),
        "короткий ключ не принимается"
    );
    let debug = format!("{parsed:?}");
    assert!(!debug.contains("AQEB"), "ключи не печатаются: {debug}");
}

#[tokio::test]
async fn passwords_are_argon2id_and_get_rehashed_when_cost_grows() {
    let weak = Passwords::new(Cost::QUICK, 2).expect("хешер");
    let password = Secret::new("correct-horse".to_string());
    let record = weak.hash(&password).await.expect("хеш");
    assert!(
        record.starts_with("$argon2id$v=19$m=64,t=1,p=1$"),
        "{record}"
    );
    let again = weak.hash(&password).await.expect("хеш");
    assert_ne!(record, again, "у каждой записи своя соль");
    assert_eq!(
        weak.verify(&password, Some(&record))
            .await
            .expect("проверка"),
        Verdict::Right { rehashed: None }
    );
    let wrong = Secret::new("battery-staple".to_string());
    assert_eq!(
        weak.verify(&wrong, Some(&record)).await.expect("проверка"),
        Verdict::Wrong
    );
    assert_eq!(
        weak.verify(&password, Some("мусор вместо записи"))
            .await
            .expect("проверка"),
        Verdict::Wrong,
        "мусор в базе это отказ, а не паника"
    );
    assert_eq!(
        weak.verify(&password, None).await.expect("проверка"),
        Verdict::Wrong,
        "для несуществующего ника считается такой же хеш по фальшивой записи"
    );
    let stronger = Passwords::new(
        Cost {
            memory_kib: 128,
            ..Cost::QUICK
        },
        2,
    )
    .expect("хешер");
    let Verdict::Right {
        rehashed: Some(upgraded),
    } = stronger
        .verify(&password, Some(&record))
        .await
        .expect("проверка")
    else {
        panic!("пароль верный, а параметры выросли: запись пересчитывается");
    };
    assert!(upgraded.contains("m=128"), "{upgraded}");
}

#[tokio::test(flavor = "current_thread")]
async fn hashing_does_not_freeze_the_runtime() {
    let passwords = Passwords::new(Cost::STRONG, 1).expect("хешер");
    let ticks = Arc::new(AtomicU64::new(0));
    let counter = Arc::clone(&ticks);
    let ticker = tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_millis(1)).await;
            counter.fetch_add(1, Ordering::SeqCst);
        }
    });
    let password = Secret::new("correct-horse".to_string());
    let record = passwords.hash(&password).await.expect("хеш");
    ticker.abort();
    assert!(record.contains("m=19456"));
    assert!(
        ticks.load(Ordering::SeqCst) > 0,
        "argon2 считается в spawn_blocking: однопоточный рантайм продолжал тикать"
    );
}
