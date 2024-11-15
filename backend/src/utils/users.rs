// /backend/src/utils/users.rs
use sqlx::Row;

#[tracing::instrument(name = "Getting an active user from the DB.", skip(executor))]
pub async fn get_active_user_from_db<'e, E>(
    executor: E,
    id: Option<uuid::Uuid>,
    email: Option<&String>,
) -> Result<crate::types::User, sqlx::Error>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let mut query_builder =
        sqlx::query_builder::QueryBuilder::new(crate::queries::USER_AND_USER_PROFILE_QUERY);

    // Add WHERE clause only if we have conditions
    if id.is_some() || email.is_some() {
        query_builder.push(" WHERE ");

        let mut first_condition = true;

        if let Some(user_id) = id {
            query_builder.push(" u.id = ");
            query_builder.push_bind(user_id);
            first_condition = false;
        }

        if let Some(user_email) = email {
            if !first_condition {
                query_builder.push(" AND ");
            }
            query_builder.push(" u.email = ");
            query_builder.push_bind(user_email);
        }
    }

    // Add active user condition
    query_builder.push(" AND u.is_active = true");

    let user = query_builder
        .build()
        .map(|row: sqlx::postgres::PgRow| crate::types::User {
            id: row.get("u_id"),
            email: row.get("u_email"),
            first_name: row.get("u_first_name"),
            password: row.get("u_password"),
            last_name: row.get("u_last_name"),
            is_active: row.get("u_is_active"),
            is_staff: row.get("u_is_staff"),
            is_superuser: row.get("u_is_superuser"),
            thumbnail: row.get("u_thumbnail"),
            date_joined: row.get("u_date_joined"),
            profile: crate::types::UserProfile {
                id: row.get("p_id"),
                user_id: row.get("p_user_id"),
                phone_number: row.get("p_phone_number"),
                birth_date: row.get("p_birth_date"),
                github_link: row.get("p_github_link"),
            },
        })
        .fetch_one(executor)
        .await;

    match user {
        Ok(user) => Ok(user),
        Err(err) => {
            tracing::error!(
                error = %err,
                "Failed to fetch active user from database"
            );
            Err(err)
        }
    }
}