// /backend/src/routes/users/login.rs
use actix_web::{post, web, HttpResponse};
use actix_session::Session;

#[derive(serde::Deserialize, Debug, serde::Serialize)]
pub struct LoginUser {
    email: String,
    password: String,
}

#[tracing::instrument(
    name = "Logging a user in",
    skip(pool, user, session),
    fields(user_email = %user.email)
)]
#[post("/login/")]
async fn login_user(
    pool: web::Data<sqlx::postgres::PgPool>,
    user: web::Json<LoginUser>,
    session: Session,
) -> HttpResponse {
    match crate::utils::get_active_user_from_db(&**pool, None, Some(&user.email)).await {
        Ok(loggedin_user) => {
            let verification_result = tokio::task::spawn_blocking(move || {
                crate::utils::verify_password(
                    loggedin_user.password.as_ref(),
                    user.password.as_bytes()
                )
            })
                .await
                .expect("Failed to complete password verification");

            match verification_result {
                Ok(_) => {
                    tracing::info!("User logged in successfully");
                    session.renew();

                    if let Err(e) = session.insert(crate::types::USER_ID_KEY, loggedin_user.id) {
                        tracing::error!("Failed to insert user_id into session: {}", e);
                        return HttpResponse::InternalServerError().json(crate::types::ErrorResponse {
                            error: "Session management error".to_string(),
                        });
                    }

                    if let Err(e) = session.insert(crate::types::USER_EMAIL_KEY, &loggedin_user.email) {
                        tracing::error!("Failed to insert user_email into session: {}", e);
                        return HttpResponse::InternalServerError().json(crate::types::ErrorResponse {
                            error: "Session management error".to_string(),
                        });
                    }

                    HttpResponse::Ok().json(crate::types::UserVisible {
                        id: loggedin_user.id,
                        email: loggedin_user.email,
                        first_name: loggedin_user.first_name,
                        last_name: loggedin_user.last_name,
                        is_active: loggedin_user.is_active,
                        is_staff: loggedin_user.is_staff,
                        is_superuser: loggedin_user.is_superuser,
                        date_joined: loggedin_user.date_joined,
                        thumbnail: loggedin_user.thumbnail,
                        profile: crate::types::UserProfile {
                            id: loggedin_user.profile.id,
                            user_id: loggedin_user.profile.user_id,
                            phone_number: loggedin_user.profile.phone_number,
                            birth_date: loggedin_user.profile.birth_date,
                            github_link: loggedin_user.profile.github_link,
                        },
                    })
                }
                Err(e) => {
                    tracing::error!(error = %e, "Failed to authenticate user");
                    HttpResponse::BadRequest().json(crate::types::ErrorResponse {
                        error: "Email and password do not match".to_string(),
                    })
                }
            }
        }
        Err(e) => {
            tracing::error!("User not found: {:#?}", e);
            HttpResponse::NotFound().json(crate::types::ErrorResponse {
                error: "A user with these details does not exist or is not active".to_string(),
            })
        }
    }
}