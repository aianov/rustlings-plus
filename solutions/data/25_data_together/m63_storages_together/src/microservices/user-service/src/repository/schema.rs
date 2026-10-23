use db_engine::{Migration, OUTBOX_CLAIMS_SQL, OUTBOX_SQL};

pub const MIGRATIONS: [Migration; 6] = [
    ("0000_outbox", OUTBOX_SQL),
    (
        "0001_init.sql",
        include_str!("../../migrations/0001_init.sql"),
    ),
    (
        "0002_counters.sql",
        include_str!("../../migrations/0002_counters.sql"),
    ),
    (
        "0003_indexes.sql",
        include_str!("../../migrations/0003_indexes.sql"),
    ),
    (
        "0004_search_settings.sql",
        include_str!("../../migrations/0004_search_settings.sql"),
    ),
    ("0005_outbox_claims", OUTBOX_CLAIMS_SQL),
];
