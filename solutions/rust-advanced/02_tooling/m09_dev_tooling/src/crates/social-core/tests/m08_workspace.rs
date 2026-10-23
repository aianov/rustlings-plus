use social_core::{DomainError, MemNetwork, PostId, UserId};

fn same_type<T>(_: T, _: T) {}

#[test]
fn ids_live_in_contracts() {
    same_type(contracts::UserId(1), UserId(1));
    same_type(contracts::PostId(1), PostId(1));
}

#[test]
fn name_rules_come_from_contracts() {
    assert_eq!(
        DomainError::from(contracts::NameError::Empty),
        DomainError::EmptyName,
        "From<NameError> для DomainError: Empty становится EmptyName"
    );
    assert_eq!(
        DomainError::from(contracts::NameError::TooLong),
        DomainError::NameTooLong,
        "From<NameError> для DomainError: TooLong становится NameTooLong"
    );
    let mut net = MemNetwork::new();
    assert_eq!(
        net.register("   "),
        Err(DomainError::EmptyName),
        "ядро проверяет ник правилами из contracts"
    );
    let alice = net
        .register(" ALICE ")
        .expect("нормальный ник регистрируется");
    let name =
        contracts::Username::try_from(" ALICE ").expect("тот же ник проходит проверку Username");
    assert_eq!(
        net.user(alice).map(|user| user.name().to_string()),
        Some(String::from(name)),
        "ядро хранит ник ровно таким, каким его делает Username"
    );
}
