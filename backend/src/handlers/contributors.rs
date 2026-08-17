use crate::{
    auth::authorized,
    error::ApiError,
    models::{Contributor, ContributorInput, PublicContributor},
    state::AppState,
};
use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use encoding_rs::GBK;
use reqwest::Client;
use serde::Serialize;
use serde_json::Value;
use std::{str, time::Duration};
use uuid::Uuid;

#[derive(Debug, Serialize)]
pub(crate) struct QqProfile {
    pub(crate) nickname: String,
    pub(crate) avatar_url: String,
}

fn validate_qq(account: &str) -> Result<(), ApiError> {
    if !(5..=12).contains(&account.len()) || !account.chars().all(|value| value.is_ascii_digit()) {
        return Err(ApiError::BadRequest("QQ 号应为 5-12 位数字".into()));
    }
    Ok(())
}

pub(crate) async fn fetch_qq_profile(account: &str) -> Result<Option<QqProfile>, ApiError> {
    validate_qq(account)?;
    let client = Client::builder()
        .timeout(Duration::from_secs(4))
        .build()
        .map_err(|_| ApiError::Unavailable)?;
    let nickname = fetch_qzone_nickname(&client, account)
        .await
        .or(fetch_fallback_qq_nickname(&client, account).await);
    Ok(nickname.map(|nickname| QqProfile {
        nickname,
        avatar_url: format!(
            "https://q.qlogo.cn/headimg_dl?dst_uin={account}&spec=640&img_type=jpg"
        ),
    }))
}

async fn fetch_qzone_nickname(client: &Client, account: &str) -> Option<String> {
    let response = client
        .get("https://users.qzone.qq.com/fcg-bin/cgi_get_portrait.fcg")
        .query(&[("uins", account)])
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?
        .bytes()
        .await
        .ok()?;
    if response
        .windows(3)
        .any(|window| window == [0xef, 0xbf, 0xbd])
    {
        return None;
    }
    let body = match str::from_utf8(&response) {
        Ok(value) => value.to_owned(),
        Err(_) => GBK.decode(&response).0.into_owned(),
    };
    let body = body.trim();
    let Some(start) = body.find('(') else {
        return None;
    };
    let Some(end) = body.rfind(')') else {
        return None;
    };
    let payload = serde_json::from_str::<Value>(&body[start + 1..end]).ok()?;
    let Some(profile) = payload.get(account).and_then(Value::as_array) else {
        return None;
    };
    profile
        .get(6)
        .and_then(Value::as_str)
        .map(str::trim)
        .map(str::to_owned)
        .filter(|value| !value.is_empty() && value.chars().count() <= 40)
}

async fn fetch_fallback_qq_nickname(client: &Client, account: &str) -> Option<String> {
    let response = client
        .get("https://api.xcvts.cn/api/qq_info")
        .query(&[("qq", account)])
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?
        .json::<Value>()
        .await
        .ok()?;
    response
        .get("data")?
        .get("name")?
        .as_str()
        .map(str::trim)
        .map(str::to_owned)
        .filter(|value| !value.is_empty() && value.chars().count() <= 40)
}

pub(crate) async fn get_qq_profile(
    Path(account): Path<String>,
) -> Result<Json<QqProfile>, ApiError> {
    fetch_qq_profile(account.trim())
        .await?
        .map(Json)
        .ok_or(ApiError::NotFound)
}

pub(crate) fn validate_and_resolve(
    input: &ContributorInput,
) -> Result<(String, String, String), ApiError> {
    let nickname = input.nickname.trim();
    let platform = input.platform.trim().to_ascii_lowercase();
    let account = input.account.trim();

    if !(1..=40).contains(&nickname.chars().count()) {
        return Err(ApiError::BadRequest("贡献者昵称应为 1-40 个字符".into()));
    }
    if !matches!(platform.as_str(), "qq" | "wechat" | "github") {
        return Err(ApiError::BadRequest(
            "联系方式类型只能是 QQ、微信或 GitHub".into(),
        ));
    }
    if account.is_empty() || account.chars().count() > 100 {
        return Err(ApiError::BadRequest("账号应为 1-100 个字符".into()));
    }
    if !(0..=10_000).contains(&input.sort_order) {
        return Err(ApiError::BadRequest("排序应为 0-10000 的整数".into()));
    }

    if platform == "qq" {
        validate_qq(account)?;
    }
    if platform == "github"
        && (!(1..=39).contains(&account.len())
            || account.starts_with('-')
            || account.ends_with('-')
            || !account
                .chars()
                .all(|value| value.is_ascii_alphanumeric() || value == '-'))
    {
        return Err(ApiError::BadRequest("GitHub 用户名格式不正确".into()));
    }

    let custom_avatar = input
        .avatar_url
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if let Some(url) = custom_avatar {
        if url.len() > 2048
            || !(url.starts_with("https://")
                || url.starts_with("http://")
                || url.starts_with("/uploads/"))
        {
            return Err(ApiError::BadRequest(
                "头像地址必须是 HTTP(S) 地址或本站上传地址".into(),
            ));
        }
    }

    let avatar_url = match (custom_avatar, platform.as_str()) {
        (Some(url), _) => url.to_owned(),
        (None, "qq") => {
            format!("https://q.qlogo.cn/headimg_dl?dst_uin={account}&spec=640&img_type=jpg")
        }
        (None, "github") => format!("https://github.com/{account}.png?size=160"),
        (None, "wechat") => "https://api.dicebear.com/9.x/initials/svg?seed=Contributor".to_owned(),
        _ => unreachable!(),
    };

    Ok((nickname.to_owned(), platform, avatar_url))
}

pub(crate) async fn list_contributors(
    State(state): State<AppState>,
) -> Result<Json<Vec<PublicContributor>>, ApiError> {
    let contributors = sqlx::query_as::<_, Contributor>(
        "SELECT id, nickname, platform, account, avatar_url, sort_order, is_visible, created_at, updated_at
         FROM contributors WHERE is_visible ORDER BY sort_order, created_at",
    )
    .fetch_all(&state.pool)
    .await?
    .into_iter()
    .map(|contributor| PublicContributor {
        id: contributor.id,
        nickname: contributor.nickname,
        profile_url: (contributor.platform == "github")
            .then(|| format!("https://github.com/{}", contributor.account)),
        platform: contributor.platform,
        avatar_url: contributor.avatar_url,
    })
    .collect();
    Ok(Json(contributors))
}

pub(crate) async fn list_admin_contributors(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<Contributor>>, ApiError> {
    authorized(&headers, &state)?;
    let contributors = sqlx::query_as::<_, Contributor>(
        "SELECT id, nickname, platform, account, avatar_url, sort_order, is_visible, created_at, updated_at
         FROM contributors ORDER BY sort_order, created_at",
    )
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(contributors))
}

pub(crate) async fn create_contributor(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<ContributorInput>,
) -> Result<(StatusCode, Json<Contributor>), ApiError> {
    authorized(&headers, &state)?;
    let (nickname, platform, avatar_url) = validate_and_resolve(&input)?;
    let contributor = sqlx::query_as::<_, Contributor>(
        "INSERT INTO contributors (id, nickname, platform, account, avatar_url, sort_order, is_visible)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         RETURNING id, nickname, platform, account, avatar_url, sort_order, is_visible, created_at, updated_at",
    )
    .bind(Uuid::new_v4())
    .bind(nickname)
    .bind(platform)
    .bind(input.account.trim())
    .bind(avatar_url)
    .bind(input.sort_order)
    .bind(input.is_visible)
    .fetch_one(&state.pool)
    .await?;
    Ok((StatusCode::CREATED, Json(contributor)))
}

pub(crate) async fn update_contributor(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(input): Json<ContributorInput>,
) -> Result<Json<Contributor>, ApiError> {
    authorized(&headers, &state)?;
    let (nickname, platform, avatar_url) = validate_and_resolve(&input)?;
    let contributor = sqlx::query_as::<_, Contributor>(
        "UPDATE contributors
         SET nickname = $2, platform = $3, account = $4, avatar_url = $5,
             sort_order = $6, is_visible = $7, updated_at = now()
         WHERE id = $1
         RETURNING id, nickname, platform, account, avatar_url, sort_order, is_visible, created_at, updated_at",
    )
    .bind(id)
    .bind(nickname)
    .bind(platform)
    .bind(input.account.trim())
    .bind(avatar_url)
    .bind(input.sort_order)
    .bind(input.is_visible)
    .fetch_optional(&state.pool)
    .await?
    .ok_or(ApiError::NotFound)?;
    Ok(Json(contributor))
}

pub(crate) async fn delete_contributor(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    authorized(&headers, &state)?;
    let result = sqlx::query("DELETE FROM contributors WHERE id = $1")
        .bind(id)
        .execute(&state.pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(ApiError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}
