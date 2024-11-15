// backend/src/routes/users/register.rs
use actix_web::{post, web, HttpResponse};
use deadpool_redis::Pool;
use sqlx::{PgPool, Row, Transaction, Postgres, Execute};
use uuid::Uuid;
use serde::Deserialize;

#[derive(Deserialize, Debug)]
struct Settings {
    database_url: String,
}

impl Settings {
    pub fn load() -> Result<Self, config::ConfigError> {
        let builder = config::Config::builder()
            .add_source(config::Environment::default().with_prefix("APP").separator("__"));
        builder.build()?.try_deserialize()
    }
}

#[derive(serde::Deserialize, Debug, serde::Serialize)]
pub struct NewUser {
    email: String,
    password: String,
    first_name: String,
    last_name: String,
}

#[derive(serde::Deserialize, serde::Serialize)]
pub struct CreateNewUser {
    email: String,
    password: String,
    first_name: String,
    last_name: String,
}

#[tracing::instrument(
    name = "Adding a new user",
    skip(pool, new_user, redis_pool),
    fields(
        new_user_email = %new_user.email,
        new_user_first_name = %new_user.first_name,
        new_user_last_name = %new_user.last_name
    )
)]
#[post("/register/")]
pub async fn register_user(
    pool: web::Data<PgPool>,
    new_user: web::Json<NewUser>,
    redis_pool: web::Data<Pool>,
) -> HttpResponse {
    let mut transaction = match pool.begin().await {
        Ok(transaction) => transaction,
        Err(e) => {
            tracing::error!("Unable to begin DB transaction: {:#?}", e);
            return HttpResponse::InternalServerError().json(
                crate::types::ErrorResponse {
                    error: "Something unexpected happened. Kindly try again.".to_string(),
                },
            );
        }
    };

    let hashed_password = crate::utils::hash(new_user.0.password.as_bytes()).await;

    let create_new_user = CreateNewUser {
        password: hashed_password,
        email: new_user.0.email,
        first_name: new_user.0.first_name,
        last_name: new_user.0.last_name,
    };

    let user_id = match insert_created_user_into_db(&mut transaction, &create_new_user).await {
        Ok(id) => id,
        Err(e) => {
            if let Some(db_error) = e.as_database_error() {
                if let Some(code) = db_error.code() {
                    if code == "23505" {
                        return HttpResponse::BadRequest().json(crate::types::ErrorResponse {
                            error: "A user with that email address already exists".to_string(),
                        });
                    }
                }
            }
            tracing::error!("Failed to insert user into DB: {:#?}", e);
            return HttpResponse::BadRequest().json(crate::types::ErrorResponse {
                error: "Error inserting user into the database".to_string(),
            });
        }
    };

    let mut redis_con = match redis_pool.get().await {
        Ok(conn) => conn,
        Err(e) => {
            tracing::error!("Failed to get Redis connection: {}", e);
            return HttpResponse::InternalServerError().json(crate::types::ErrorResponse {
                error: "We cannot activate your account at the moment".to_string(),
            });
        }
    };

    if let Err(e) = crate::utils::send_multipart_email(
        "RustAuth - Let's get you verified".to_string(),
        user_id,
        create_new_user.email.clone(),
        create_new_user.first_name,
        create_new_user.last_name,
        "verification_email.html",
        &mut redis_con,
    )
        .await
    {
        tracing::error!("Failed to send verification email: {}", e);
        return HttpResponse::InternalServerError().json(crate::types::ErrorResponse {
            error: "Account created but verification email could not be sent".to_string(),
        });
    }

    if let Err(e) = transaction.commit().await {
        tracing::error!("Failed to commit transaction: {}", e);
        return HttpResponse::InternalServerError().json(crate::types::ErrorResponse {
            error: "Failed to complete registration".to_string(),
        });
    }

    tracing::info!("User created successfully");
    HttpResponse::Ok().json(crate::types::SuccessResponse {
        message: "Your account was created successfully. Check your email address to activate your account as we just sent you an activation link. Ensure you activate your account before the link expires".to_string(),
    })
}

#[tracing::instrument(
    name = "Inserting new user into DB",
    skip(transaction, new_user),
    fields(
        new_user_email = %new_user.email,
        new_user_first_name = %new_user.first_name,
        new_user_last_name = %new_user.last_name
    )
)]
async fn insert_created_user_into_db<'a>(
    transaction: &mut Transaction<'_, Postgres>,
    new_user: &CreateNewUser,
) -> Result<Uuid, sqlx::Error> {
    let rec = match sqlx::query!(
        r#"
        INSERT INTO users (email, password, first_name, last_name)
        VALUES ($1, $2, $3, $4)
        RETURNING id
        "#,
        new_user.email,
        new_user.password,
        new_user.first_name,
        new_user.last_name
    )
        .fetch_one(transaction)
        .await {
        Ok(record) => record,
        Err(err) => {
            tracing::error!("Error during user insertion: {}", err);
            return Err(err);
        }
    };

    let user_id = rec.id;

    let profile = sqlx::query!(
        r#"
        INSERT INTO user_profile (user_id)
        VALUES ($1)
        ON CONFLICT (user_id) DO NOTHING
        RETURNING user_id
        "#,
        user_id
    )
        .fetch_one(transaction)
        .await?;

    tracing::info!("User profile created successfully for user {}", profile.user_id);
    Ok(profile.user_id)
}
