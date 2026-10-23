CREATE TABLE notices (
    id BIGINT GENERATED ALWAYS AS IDENTITY,
    to_user BIGINT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('followed', 'liked', 'commented', 'new_post', 'digest')),
    actor BIGINT NOT NULL,
    post BIGINT,
    count INTEGER,
    read BOOLEAN NOT NULL DEFAULT false,
    at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (id, at)
) PARTITION BY RANGE (at);

CREATE INDEX notices_by_user ON notices (to_user, id DESC);
CREATE INDEX notices_unread ON notices (to_user) WHERE NOT read;

CREATE FUNCTION notices_ensure_partitions(back INTEGER, ahead INTEGER) RETURNS INTEGER
LANGUAGE plpgsql SECURITY DEFINER SET search_path FROM CURRENT AS $$
DECLARE
    month DATE;
    made INTEGER := 0;
BEGIN
    FOR step IN -back..ahead LOOP
        month := (date_trunc('month', now()) + make_interval(months => step))::date;
        IF to_regclass(format('notices_%s', to_char(month, 'YYYY_MM'))) IS NULL THEN
            EXECUTE format(
                'CREATE TABLE %I PARTITION OF notices FOR VALUES FROM (%L) TO (%L)',
                format('notices_%s', to_char(month, 'YYYY_MM')),
                month,
                (month + interval '1 month')::date
            );
            made := made + 1;
        END IF;
    END LOOP;
    RETURN made;
END $$;

CREATE FUNCTION notices_drop_older(keep INTEGER) RETURNS INTEGER
LANGUAGE plpgsql SECURITY DEFINER SET search_path FROM CURRENT AS $$
DECLARE
    part RECORD;
    edge DATE := (date_trunc('month', now()) - make_interval(months => keep))::date;
    dropped INTEGER := 0;
BEGIN
    FOR part IN
        SELECT c.relname
        FROM pg_inherits i JOIN pg_class c ON c.oid = i.inhrelid
        WHERE i.inhparent = 'notices'::regclass
    LOOP
        IF to_date(right(part.relname, 7), 'YYYY_MM') < edge THEN
            EXECUTE format('DROP TABLE %I', part.relname);
            dropped := dropped + 1;
        END IF;
    END LOOP;
    RETURN dropped;
END $$;

SELECT notices_ensure_partitions(0, 2);
