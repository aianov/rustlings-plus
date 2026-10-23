DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'social_app') THEN
        CREATE ROLE social_app LOGIN PASSWORD 'social_app';
    END IF;
END $$;

ALTER ROLE social_app SET statement_timeout = '2s';
ALTER ROLE social_app SET idle_in_transaction_session_timeout = '10s';
GRANT CONNECT ON DATABASE rustlings TO social_app;

DO $$
DECLARE
    schema_name TEXT;
BEGIN
    FOREACH schema_name IN ARRAY ARRAY['auth', 'users', 'posts', 'notify'] LOOP
        EXECUTE format('CREATE SCHEMA IF NOT EXISTS %I', schema_name);
        EXECUTE format('GRANT USAGE ON SCHEMA %I TO social_app', schema_name);
        EXECUTE format('GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA %I TO social_app', schema_name);
        EXECUTE format('GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA %I TO social_app', schema_name);
        EXECUTE format('GRANT EXECUTE ON ALL FUNCTIONS IN SCHEMA %I TO social_app', schema_name);
        EXECUTE format('ALTER DEFAULT PRIVILEGES IN SCHEMA %I GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO social_app', schema_name);
        EXECUTE format('ALTER DEFAULT PRIVILEGES IN SCHEMA %I GRANT USAGE, SELECT ON SEQUENCES TO social_app', schema_name);
        EXECUTE format('ALTER DEFAULT PRIVILEGES IN SCHEMA %I GRANT EXECUTE ON FUNCTIONS TO social_app', schema_name);
    END LOOP;
END $$;
