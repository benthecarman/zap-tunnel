CREATE TABLE users
(
    username TEXT PRIMARY KEY NOT NULL,
    auth_key TEXT             NOT NULL
);

CREATE TABLE invoices
(
    payment_hash TEXT PRIMARY KEY NOT NULL,
    invoice      TEXT UNIQUE      NOT NULL,
    paid         INTEGER          NOT NULL,
    username     TEXT             NOT NULL REFERENCES users (username)
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