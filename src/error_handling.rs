use warp::{Rejection, Reply, reject};
use warp::http::StatusCode;
use log::warn;
use serde::{Serialize};


#[derive(Serialize)]
pub struct AuthError {
    msg: String,
    token: String,
}

#[derive(Debug)]
pub struct AuthRejection {
    pub token: String
}

impl reject::Reject for AuthRejection {}

pub async fn error_handler(err: Rejection) -> Result<impl Reply, Rejection> {
    if err.is_not_found() {
        Ok(StatusCode::NOT_FOUND)
    } else if let Some(AuthRejection { token }) = err.find() {
        warn!("{}", serde_json::to_string(&AuthError {
            msg: String::from("authentication error"),
            token: String::from(token),
        }).unwrap());

        Ok(StatusCode::UNAUTHORIZED)
    } else {
        Ok(StatusCode::INTERNAL_SERVER_ERROR)
    }
}