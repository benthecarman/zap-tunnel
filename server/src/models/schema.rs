// @generated automatically by Diesel CLI.

diesel::table! {
    invoices (payment_hash) {
        payment_hash -> Text,
        invoice -> Text,
        expires_at -> BigInt,
        paid -> Integer,
        username -> Text,
    }
}

diesel::table! {
    users (username) {
        username -> Text,
        pubkey -> Text,
    }
}

diesel::table! {
    zaps (payment_hash) {
        payment_hash -> Text,
        invoice -> Text,
        request -> Text,
        note_id -> Nullable<Text>,
    }
}

diesel::joinable!(invoices -> users (username));

diesel::allow_tables_to_appear_in_same_query!(invoices, users, zaps,);
