use axum::http::StatusCode;
use axum::{Extension, Json};
use bitcoin::secp256k1::SECP256K1;
use diesel::{RunQueryDsl, SqliteConnection};
pub use zap_tunnel_client::CreateUser;

use crate::models::schema::*;
use crate::models::user::User;
use crate::routes::handle_anyhow_error;
use crate::State;

pub(crate) fn create_user_impl(
    payload: CreateUser,
    connection: &mut SqliteConnection,
) -> anyhow::Result<User> {
    // validate username and signature
    payload.validate(SECP256K1)?;

    let new_user = User::new(&payload.username, payload.pubkey()?);

    // create user
    let num_created: usize = diesel::insert_into(users::dsl::users)
        .values(&new_user)
        .execute(connection)?;

    debug_assert!(num_created == 1);

    println!("New user created! {:?}", new_user);

    Ok(new_user)
}

pub async fn create_user(
    Extension(state): Extension<State>,
    Json(payload): Json<CreateUser>,
) -> Result<Json<User>, (StatusCode, String)> {
    let mut connection = state.db_pool.get().unwrap();

    match create_user_impl(payload, &mut connection) {
        Ok(res) => Ok(Json(res)),
        Err(e) => Err(handle_anyhow_error(e)),
    }
}
