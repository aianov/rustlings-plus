use db_engine::{Migration, OUTBOX_SQL};

pub const MIGRATIONS: [Migration; 5] = [
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
];
