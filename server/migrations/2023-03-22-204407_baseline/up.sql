CREATE TABLE users
(
    username TEXT PRIMARY KEY NOT NULL,
    pubkey   TEXT UNIQUE      NOT NULL
);

CREATE TABLE invoices
(
    payment_hash   TEXT PRIMARY KEY NOT NULL,
    invoice        TEXT UNIQUE      NOT NULL,
    expires_at     BIGINT           NOT NULL,
    wrapped_expiry BIGINT,
    paid           INTEGER          NOT NULL,
    username       TEXT             NOT NULL REFERENCES users (username)
);

create index invoices_paid_idx on invoices (paid);
create index invoices_username_idx on invoices (username);

CREATE TABLE zaps
(
    payment_hash TEXT PRIMARY KEY NOT NULL,
    invoice      TEXT UNIQUE      NOT NULL,
    request      TEXT             NOT NULL,
    note_id      TEXT UNIQUE
);