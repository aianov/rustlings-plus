use db_engine::{Migration, OUTBOX_CLAIMS_SQL, OUTBOX_SQL};

pub const MIGRATIONS: [Migration; 13] = [
    ("0000_outbox", OUTBOX_SQL),
    (
        "0001_init.sql",
        include_str!("../../migrations/0001_init.sql"),
    ),
    (
        "0002_threads.sql",
        include_str!("../../migrations/0002_threads.sql"),
    ),
    (
        "0003_pins.sql",
        include_str!("../../migrations/0003_pins.sql"),
    ),
    (
        "0004_indexes.sql",
        include_str!("../../migrations/0004_indexes.sql"),
    ),
    (
        "0005_body_expand.sql",
        include_str!("../../migrations/0005_body_expand.sql"),
    ),
    (
        "0006_body_backfill.sql",
        include_str!("../../migrations/0006_body_backfill.sql"),
    ),
    (
        "0007_body_check.sql",
        include_str!("../../migrations/0007_body_check.sql"),
    ),
    (
        "0008_body_validate.sql",
        include_str!("../../migrations/0008_body_validate.sql"),
    ),
    (
        "0009_body_contract.sql",
        include_str!("../../migrations/0009_body_contract.sql"),
    ),
    (
        "0010_recent_index.sql",
        include_str!("../../migrations/0010_recent_index.sql"),
    ),
    (
        "0011_search_trends.sql",
        include_str!("../../migrations/0011_search_trends.sql"),
    ),
    ("0012_outbox_claims", OUTBOX_CLAIMS_SQL),
];
