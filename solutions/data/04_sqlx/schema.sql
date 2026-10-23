-- схема сервиса уведомлений: файл можно запускать сколько угодно раз
CREATE TABLE IF NOT EXISTS subscribers (
    id      bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    channel text NOT NULL DEFAULT 'email; push',
    address text NOT NULL
);

CREATE INDEX IF NOT EXISTS subscribers_address_idx ON subscribers (address);

CREATE TABLE IF NOT EXISTS deliveries (
    id            bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    subscriber_id bigint NOT NULL REFERENCES subscribers (id) ON DELETE CASCADE,
    sent_at       timestamptz
)
