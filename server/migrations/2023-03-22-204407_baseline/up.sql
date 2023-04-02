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
    fees_earned    BIGINT,
    username       TEXT,
    FOREIGN KEY (username) REFERENCES users (username)
);

create index invoices_fees_earned_idx on invoices (fees_earned);
create index invoices_username_idx on invoices (username);

CREATE TABLE zaps
(
    payment_hash TEXT PRIMARY KEY NOT NULL,
    invoice      TEXT UNIQUE      NOT NULL,
    request      TEXT             NOT NULL,
    note_id      TEXT UNIQUE
);