use crate::{
    auth::authorized,
    error::ApiError,
    models::{ArticleCategory, ArticleCategoryInput},
    state::AppState,
};
use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    Json,
};

fn validate_category(input: &ArticleCategoryInput) -> Result<(), ApiError> {
    let slug = input.slug.trim();
    if !(2..=60).contains(&slug.len())
        || !slug.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
        })
    {
        return Err(ApiError::BadRequest(
            "分类标识只能使用 2-60 位小写字母、数字和连字符".into(),
        ));
    }
    if !(2..=30).contains(&input.name.trim().chars().count()) {
        return Err(ApiError::BadRequest("分类名称应为 2-30 个字符".into()));
    }
    if !(0..=10_000).contains(&input.sort_order) {
        return Err(ApiError::BadRequest("分类排序应为 0-10000 的整数".into()));
    }
    Ok(())
}

pub(crate) async fn list_categories(
    State(state): State<AppState>,
) -> Result<Json<Vec<ArticleCategory>>, ApiError> {
    let categories = sqlx::query_as::<_, ArticleCategory>(
        "SELECT slug, name, sort_order, is_hidden FROM article_categories
         WHERE NOT is_hidden ORDER BY sort_order, name",
    )
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(categories))
}

pub(crate) async fn list_admin_categories(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<ArticleCategory>>, ApiError> {
    authorized(&headers, &state)?;
    let categories = sqlx::query_as::<_, ArticleCategory>(
        "SELECT slug, name, sort_order, is_hidden FROM article_categories
         ORDER BY sort_order, name",
    )
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(categories))
}

pub(crate) async fn create_category(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<ArticleCategoryInput>,
) -> Result<(StatusCode, Json<ArticleCategory>), ApiError> {
    authorized(&headers, &state)?;
    validate_category(&input)?;
    let category = sqlx::query_as::<_, ArticleCategory>(
        "INSERT INTO article_categories (slug, name, sort_order, is_hidden) VALUES ($1, $2, $3, $4)
         RETURNING slug, name, sort_order, is_hidden",
    )
    .bind(input.slug.trim())
    .bind(input.name.trim())
    .bind(input.sort_order)
    .bind(input.is_hidden)
    .fetch_one(&state.pool)
    .await?;
    Ok((StatusCode::CREATED, Json(category)))
}

pub(crate) async fn update_category(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(previous_slug): Path<String>,
    Json(input): Json<ArticleCategoryInput>,
) -> Result<Json<ArticleCategory>, ApiError> {
    authorized(&headers, &state)?;
    validate_category(&input)?;
    let category = sqlx::query_as::<_, ArticleCategory>(
        "UPDATE article_categories SET slug = $2, name = $3, sort_order = $4, is_hidden = $5
         WHERE slug = $1 RETURNING slug, name, sort_order, is_hidden",
    )
    .bind(previous_slug)
    .bind(input.slug.trim())
    .bind(input.name.trim())
    .bind(input.sort_order)
    .bind(input.is_hidden)
    .fetch_optional(&state.pool)
    .await?
    .ok_or(ApiError::NotFound)?;
    Ok(Json(category))
}

pub(crate) async fn delete_category(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(slug): Path<String>,
) -> Result<StatusCode, ApiError> {
    authorized(&headers, &state)?;

    let exists = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM article_categories WHERE slug = $1)",
    )
    .bind(&slug)
    .fetch_one(&state.pool)
    .await?;
    if !exists {
        return Err(ApiError::NotFound);
    }

    let (article_count, submission_count) = sqlx::query_as::<_, (i64, i64)>(
        "SELECT
            (SELECT COUNT(*) FROM articles WHERE category = $1),
            (SELECT COUNT(*) FROM submissions WHERE category = $1)",
    )
    .bind(&slug)
    .fetch_one(&state.pool)
    .await?;
    if article_count > 0 || submission_count > 0 {
        return Err(ApiError::Conflict(format!(
            "该分类仍关联 {article_count} 篇文章和 {submission_count} 条投稿，请先调整这些内容的分类"
        )));
    }

    let result = sqlx::query("DELETE FROM article_categories WHERE slug = $1")
        .bind(slug)
        .execute(&state.pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(ApiError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}
