-- Secret setting values (step 1.10, plan §5.1; NTF-004): ciphertext sealed with the instance
-- key from the environment (trackflow-domain `secret`), separate from the open `settings` table
-- so neither a database leak nor an audit event exposes them.

create table setting_secrets (
    key text primary key,
    value bytea not null
);
