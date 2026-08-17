use crate::{
    auth::authorized,
    error::ApiError,
    handlers::contributors::{fetch_qq_profile, validate_and_resolve},
    models::{
        ContributorInput, StatusUpdate, Submission, SubmissionInput, SubmissionQuery,
        SubmissionReceipt,
    },
    state::AppState,
};
use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use chrono::{Datelike, Utc};
use uuid::Uuid;

pub(crate) async fn create_submission(
    State(state): State<AppState>,
    Json(input): Json<SubmissionInput>,
) -> Result<(StatusCode, Json<SubmissionReceipt>), ApiError> {
    if input.website.as_deref().unwrap_or("").trim().is_empty() == false {
        return Err(ApiError::BadRequest("提交内容未通过校验".into()));
    }
    if !input.consent {
        return Err(ApiError::BadRequest("请确认内容已获得必要授权".into()));
    }
    if !(2..=120).contains(&input.title.trim().chars().count()) {
        return Err(ApiError::BadRequest("标题长度应为 2-120 个字符".into()));
    }
    if !(20..=500_000).contains(&input.body_markdown.trim().chars().count()) {
        return Err(ApiError::BadRequest("正文长度应为 20-500000 个字符".into()));
    }
    let category_exists = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM article_categories WHERE slug = $1 AND NOT is_hidden)",
    )
    .bind(&input.category)
    .fetch_one(&state.pool)
    .await?;
    if !category_exists {
        return Err(ApiError::BadRequest("内容分类不存在".into()));
    }

    let contributor = match (
        input
            .contributor_platform
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty()),
        input
            .contributor_account
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty()),
    ) {
        (None, None) => None,
        (Some(platform), Some(account)) => {
            let provided_nickname = input
                .contributor_nickname
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned);
            let fetched_qq_nickname =
                if platform.eq_ignore_ascii_case("qq") && provided_nickname.is_none() {
                    fetch_qq_profile(account)
                        .await
                        .ok()
                        .flatten()
                        .map(|profile| profile.nickname)
                } else {
                    None
                };
            let nickname = provided_nickname
                .or(fetched_qq_nickname)
                .unwrap_or_else(|| match platform {
                    "github" => account.to_owned(),
                    "qq" => "QQ 贡献者".to_owned(),
                    _ => "微信贡献者".to_owned(),
                });
            let contributor_input = ContributorInput {
                nickname: nickname.clone(),
                platform: platform.to_owned(),
                account: account.to_owned(),
                avatar_url: input.contributor_avatar_url.clone(),
                sort_order: 0,
                is_visible: true,
            };
            let (_, normalized_platform, avatar_url) = validate_and_resolve(&contributor_input)?;
            Some((
                normalized_platform,
                account.to_owned(),
                nickname,
                avatar_url,
            ))
        }
        _ => return Err(ApiError::BadRequest("请同时填写贡献平台和账号".into())),
    };

    let id = Uuid::new_v4();
    let compact = id.simple().to_string();
    let reference_code = format!("IIE-{}", compact[..8].to_uppercase());
    sqlx::query(
        "INSERT INTO submissions (id, reference_code, title, category, year, background, contact, body_markdown, status,
            contributor_platform, contributor_account, contributor_nickname, contributor_avatar_url)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'pending', $9, $10, $11, $12)",
    )
    .bind(id)
    .bind(&reference_code)
    .bind(input.title.trim())
    .bind(input.category)
    .bind(input.year.unwrap_or_else(|| Utc::now().year()))
    .bind(input.background.map(|value| value.trim().to_owned()))
    .bind(input.contact.map(|value| value.trim().to_owned()))
    .bind(input.body_markdown.trim())
    .bind(contributor.as_ref().map(|value| &value.0))
    .bind(contributor.as_ref().map(|value| &value.1))
    .bind(contributor.as_ref().map(|value| &value.2))
    .bind(contributor.as_ref().map(|value| &value.3))
    .execute(&state.pool)
    .await?;
    Ok((
        StatusCode::CREATED,
        Json(SubmissionReceipt { reference_code }),
    ))
}

pub(crate) async fn list_submissions(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(params): Query<SubmissionQuery>,
) -> Result<Json<Vec<Submission>>, ApiError> {
    authorized(&headers, &state)?;
    let submissions = sqlx::query_as::<_, Submission>(
        "SELECT id, reference_code, title, category, year, background, contact, body_markdown, status, created_at, reviewed_at, published_article_id,
            contributor_platform, contributor_account, contributor_nickname, contributor_avatar_url
         FROM submissions WHERE ($1::text IS NULL OR status = $1) ORDER BY created_at DESC LIMIT 200",
    )
    .bind(params.status)
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(submissions))
}

pub(crate) async fn update_submission(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(input): Json<StatusUpdate>,
) -> Result<Json<Submission>, ApiError> {
    authorized(&headers, &state)?;
    if !["pending", "approved", "rejected"].contains(&input.status.as_str()) {
        return Err(ApiError::BadRequest("审核状态不合法".into()));
    }
    let mut transaction = state.pool.begin().await?;
    let submission = sqlx::query_as::<_, Submission>(
        "UPDATE submissions SET status = $2, reviewed_at = CASE WHEN $2 = 'pending' THEN NULL ELSE now() END
         WHERE id = $1
         RETURNING id, reference_code, title, category, year, background, contact, body_markdown, status, created_at, reviewed_at, published_article_id,
            contributor_platform, contributor_account, contributor_nickname, contributor_avatar_url",
    )
    .bind(id)
    .bind(&input.status)
    .fetch_optional(&mut *transaction)
    .await?
    .ok_or(ApiError::NotFound)?;

    if input.status == "approved" {
        let article_id = submission.published_article_id.unwrap_or_else(Uuid::new_v4);
        let slug = format!("submission-{}", article_id.simple());
        let excerpt = submission.background.clone();
        sqlx::query(
            "INSERT INTO articles (id, slug, title, excerpt, body_markdown, category, year, status, published_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, 'published', now())
             ON CONFLICT (id) DO UPDATE SET title = EXCLUDED.title, excerpt = EXCLUDED.excerpt, body_markdown = EXCLUDED.body_markdown, category = EXCLUDED.category, year = EXCLUDED.year, status = 'published', updated_at = now(), published_at = COALESCE(articles.published_at, now())",
        )
        .bind(article_id)
        .bind(slug)
        .bind(&submission.title)
        .bind(excerpt)
        .bind(&submission.body_markdown)
        .bind(&submission.category)
        .bind(submission.year)
        .execute(&mut *transaction)
        .await?;
        sqlx::query("UPDATE submissions SET published_article_id = $2 WHERE id = $1")
            .bind(id)
            .bind(article_id)
            .execute(&mut *transaction)
            .await?;
        if let (Some(platform), Some(account), Some(nickname), Some(avatar_url)) = (
            submission.contributor_platform.as_deref(),
            submission.contributor_account.as_deref(),
            submission.contributor_nickname.as_deref(),
            submission.contributor_avatar_url.as_deref(),
        ) {
            let contributor_id = sqlx::query_scalar::<_, Uuid>(
                "INSERT INTO contributors (id, nickname, platform, account, avatar_url, sort_order, is_visible)
                 VALUES ($1, $2, $3, $4, $5, 0, TRUE)
                 ON CONFLICT (platform, account) DO UPDATE SET nickname = EXCLUDED.nickname, avatar_url = EXCLUDED.avatar_url, is_visible = TRUE, updated_at = now()
                 RETURNING id",
            )
            .bind(Uuid::new_v4())
            .bind(nickname)
            .bind(platform)
            .bind(account)
            .bind(avatar_url)
            .fetch_one(&mut *transaction)
            .await?;
            sqlx::query("UPDATE articles SET contributor_id = $2 WHERE id = $1")
                .bind(article_id)
                .bind(contributor_id)
                .execute(&mut *transaction)
                .await?;
        }
    } else if let Some(article_id) = submission.published_article_id {
        sqlx::query("UPDATE articles SET status = 'archived', updated_at = now() WHERE id = $1")
            .bind(article_id)
            .execute(&mut *transaction)
            .await?;
    }

    transaction.commit().await?;
    Ok(Json(submission))
}
