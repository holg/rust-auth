// backend/src/routes/users/password_change/request_change.rs
use actix_web::{post, web, HttpResponse};
use deadpool_redis::Pool;

#[derive(serde::Deserialize, Debug)]
pub struct UserEmail {
    email: String,
}

#[tracing::instrument(
    name = "Requesting a password change",
    skip(pool, redis_pool),
    fields(user_email = %user_email.0.email)
)]
#[post("/request-password-change/")]
pub async fn request_password_change(
    pool: web::Data<sqlx::postgres::PgPool>,
    user_email: web::Json<UserEmail>,
    redis_pool: web::Data<Pool>,
) -> HttpResponse {
    let settings = crate::settings::get_settings()
        .expect("Failed to read settings.");

    match crate::utils::get_active_user_from_db(&**pool, None, Some(&user_email.0.email)).await {
        Ok(visible_user_detail) => {
            let mut redis_con = match redis_pool.get().await {
                Ok(conn) => conn,
                Err(e) => {
                    tracing::error!("Failed to get Redis connection: {}", e);
                    return HttpResponse::InternalServerError().json(
                        crate::types::ErrorResponse {
                            error: "Something happened. Please try again".to_string(),
                        },
                    );
                }
            };

            if let Err(e) = crate::utils::send_multipart_email(
                "RustAuth - Password Reset Instructions".to_string(),
                visible_user_detail.id,
                visible_user_detail.email,
                visible_user_detail.first_name,
                visible_user_detail.last_name,
                "password_reset_email.html",
                &mut redis_con,
            )
                .await
            {
                tracing::error!("Failed to send password reset email: {}", e);
                return HttpResponse::InternalServerError().json(
                    crate::types::ErrorResponse {
                        error: "Failed to send password reset instructions".to_string(),
                    },
                );
            }

            HttpResponse::Ok().json(crate::types::SuccessResponse {
                message: "Password reset instructions have been sent to your email address. Kindly take action before its expiration".to_string(),
            })
        }
        Err(e) => {
            tracing::error!("User not found: {:#?}", e);
            HttpResponse::NotFound().json(crate::types::ErrorResponse {
                error: format!(
                    "An active user with this e-mail address does not exist. If you registered with this email, \
                    ensure you have activated your account. You can check by logging in. If you have not activated it, \
                    visit {}/auth/regenerate-token to regenerate the token that will allow you activate your account.",
                    settings.frontend_url
                ),
            })
        }
    }
}