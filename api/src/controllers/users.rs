use crate::auth::user::User as AuthUser;
use crate::communications::mailers;
use crate::controllers::auth;
use crate::controllers::auth::LoginRequest;
use crate::database::Connection;
use crate::errors::*;
use crate::extractors::*;
use crate::helpers::application;
use crate::models::*;
use crate::server::AppState;
use crate::server::GetAppState;
use crate::utils::google_recaptcha;
use ::rand::rngs::OsRng;
use ::rand::RngCore;
use actix_web;
use actix_web::Responder;
use actix_web::{
    http::StatusCode,
    web::{Data, Path, Query},
    HttpRequest, HttpResponse,
};
use chrono::Duration;
use db::prelude::*;
use diesel::PgConnection;
use futures::future::{err, ok, Ready};
use serde_json::Value;
use std::collections::HashMap;
use std::str;
use uuid::Uuid;

#[derive(Deserialize)]
pub struct SearchUserByEmail {
    pub email: String,
}

#[derive(Serialize)]
pub struct CurrentUser {
    pub user: DisplayUser,
    pub roles: Vec<Roles>,
    pub scopes: Vec<Scopes>,
    pub organization_roles: HashMap<Uuid, Vec<Roles>>,
    pub organization_scopes: HashMap<Uuid, Vec<Scopes>>,
    pub organization_event_ids: HashMap<Uuid, Vec<Uuid>>,
    pub organization_readonly_event_ids: HashMap<Uuid, Vec<Uuid>>,
    pub event_scopes: HashMap<Uuid, Vec<Scopes>>,
}

impl Responder for CurrentUser {
    type Future = Ready<Result<HttpResponse, actix_web::Error>>;
    type Error = actix_web::Error;

    fn respond_to(self, _req: &HttpRequest) -> Self::Future {
        match serde_json::to_string(&self) {
            Ok(body) => ok(HttpResponse::Ok().content_type("application/json").body(body)),
            Err(e) => err(e.into()),
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
///struct used to indicate paging information and search query information
pub struct ActivityParameters {
    past_or_upcoming: Option<String>,
}

#[derive(Deserialize, Clone)]
pub struct InputPushNotificationTokens {
    pub token_source: String,
    pub token: String,
}

pub async fn current_user((connection, auth_user): (Connection, AuthUser)) -> Result<CurrentUser, ApiError> {
    let connection = connection.get();
    current_user_from_user(&auth_user.user, connection)
}

pub async fn activity(
    (connection, path, query, activity_query, auth_user): (
        Connection,
        Path<OrganizationFanPathParameters>,
        Query<PagingParameters>,
        Query<ActivityParameters>,
        AuthUser,
    ),
) -> Result<WebPayload<ActivitySummary>, ApiError> {
    let connection = connection.get();
    let organization = Organization::find(path.id, connection)?;
    auth_user.requires_scope_for_organization(Scopes::OrgFans, &organization, &connection)?;
    let user = User::find(path.user_id, connection)?;

    let past_or_upcoming = match activity_query
        .past_or_upcoming
        .clone()
        .unwrap_or("upcoming".to_string())
        .as_str()
    {
        "past" => PastOrUpcoming::Past,
        _ => PastOrUpcoming::Upcoming,
    };

    let mut payload = user.activity(
        &organization,
        query.page(),
        query.limit(),
        query.dir.unwrap_or(SortingDir::Desc),
        past_or_upcoming,
        match query.get_tag_as_str("type") {
            Some(t) => Some(t.parse()?),
            None => None,
        },
        connection,
    )?;
    payload
        .paging
        .tags
        .insert("past_or_upcoming".to_string(), json!(activity_query.past_or_upcoming));

    Ok(WebPayload::new(StatusCode::OK, payload))
}

pub async fn profile(
    (connection, path, auth_user): (Connection, Path<OrganizationFanPathParameters>, AuthUser),
) -> Result<HttpResponse, ApiError> {
    let connection = connection.get();
    let organization = Organization::find(path.id, connection)?;
    auth_user.requires_scope_for_organization(Scopes::OrgFans, &organization, &connection)?;

    let user = User::find(path.user_id, connection)?;

    // Confirm organization has specified user as a fan
    if !organization.has_fan(&user, connection)? {
        return application::forbidden("Fan does not belong to this organization");
    }

    Ok(HttpResponse::Ok().json(&user.get_profile_for_organization(&organization, connection)?))
}

pub async fn history(
    (connection, path, query, auth_user): (
        Connection,
        Path<OrganizationFanPathParameters>,
        Query<PagingParameters>,
        AuthUser,
    ),
) -> Result<WebPayload<HistoryItem>, ApiError> {
    let connection = connection.get();
    let organization = Organization::find(path.id, connection)?;
    auth_user.requires_scope_for_organization(Scopes::OrgFans, &organization, &connection)?;

    let user = User::find(path.user_id, connection)?;

    // Confirm organization has specified user as a fan
    if !organization.has_fan(&user, connection)? {
        return application::unauthorized(Some(auth_user), None);
    }

    let payload = user.get_history_for_organization(
        &organization,
        query.page(),
        query.limit(),
        query.dir.unwrap_or(SortingDir::Desc),
        connection,
    )?;

    Ok(WebPayload::new(StatusCode::OK, payload))
}

pub async fn update_current_user(
    (connection, user_parameters, auth_user): (Connection, Json<UserProfileAttributes>, AuthUser),
) -> Result<CurrentUser, ApiError> {
    let connection = connection.get();

    let updated_user = auth_user
        .user
        .update(user_parameters.into_inner().into(), Some(auth_user.id()), connection)?;
    let current_user = current_user_from_user(&updated_user, connection)?;
    Ok(current_user)
}

pub async fn show(
    (connection, parameters, auth_user): (Connection, Path<PathParameters>, AuthUser),
) -> Result<HttpResponse, ApiError> {
    let connection = connection.get();
    let user = User::find(parameters.id, connection)?;
    if !(auth_user.user == user || auth_user.user.is_admin()) {
        return application::unauthorized(Some(auth_user), None);
    }

    Ok(HttpResponse::Ok().json(&user.for_display()?))
}

pub async fn list_organizations(
    (connection, parameters, query_parameters, auth_user): (
        Connection,
        Path<PathParameters>,
        Query<PagingParameters>,
        AuthUser,
    ),
) -> Result<HttpResponse, ApiError> {
    let connection = connection.get();
    let user = User::find(parameters.id, connection)?;
    if !(auth_user.user == user || auth_user.user.is_admin()) {
        return application::unauthorized(Some(auth_user), None);
    }
    //TODO implement proper paging on db.
    let organization_links = Organization::all_org_names_linked_to_user(parameters.id, connection)?;

    Ok(HttpResponse::Ok().json(&Payload::from_data(
        organization_links,
        query_parameters.page(),
        query_parameters.limit(),
        None,
    )))
}

pub async fn show_push_notification_tokens_for_user_id(
    (connection, parameters, auth_user): (Connection, Path<PathParameters>, AuthUser),
) -> Result<HttpResponse, ApiError> {
    let connection = connection.get();
    let user = User::find(parameters.id, connection)?;
    if !(auth_user.user == user || auth_user.user.is_admin()) {
        return application::unauthorized(Some(auth_user), None);
    }

    let push_notification_tokens: Vec<DisplayPushNotificationToken> =
        PushNotificationToken::find_by_user_id(parameters.id, connection)?
            .iter()
            .map(|t| DisplayPushNotificationToken::from(t.clone()))
            .collect();

    Ok(HttpResponse::Ok().json(&push_notification_tokens))
}

pub async fn show_push_notification_tokens(
    (connection, auth_user): (Connection, AuthUser),
) -> Result<HttpResponse, ApiError> {
    let connection = connection.get();

    let push_notification_tokens: Vec<DisplayPushNotificationToken> =
        PushNotificationToken::find_by_user_id(auth_user.user.id, connection)?
            .iter()
            .map(|t| DisplayPushNotificationToken::from(t.clone()))
            .collect();

    Ok(HttpResponse::Ok().json(&push_notification_tokens))
}

pub async fn add_push_notification_token(
    (connection, add_request, auth_user): (Connection, Json<InputPushNotificationTokens>, AuthUser),
) -> Result<HttpResponse, ApiError> {
    let connection = connection.get();

    let add_push_notification_token_request = add_request.into_inner();
    PushNotificationToken::create(
        auth_user.user.id,
        add_push_notification_token_request.token_source.clone(),
        add_push_notification_token_request.token.clone(),
    )
    .commit(auth_user.id(), connection)?;

    Ok(HttpResponse::Ok().finish())
}

pub async fn remove_push_notification_token(
    (connection, parameters, auth_user): (Connection, Path<PathParameters>, AuthUser),
) -> Result<HttpResponse, ApiError> {
    let connection = connection.get();

    PushNotificationToken::remove(auth_user.user.id, parameters.id, connection)?;

    Ok(HttpResponse::Ok().finish())
}

pub async fn register(
    (http_request, connection, parameters): (HttpRequest, Connection, Json<RegisterRequest>),
) -> Result<HttpResponse, ApiError> {
    let state = http_request.state();
    let connection_info = http_request.connection_info();
    let remote_ip = connection_info.remote();
    let mut log_data = HashMap::new();
    log_data.insert("email", parameters.email.clone().into());

    if let Some(ref google_recaptcha_secret_key) = state.config.google_recaptcha_secret_key {
        if let Err(err) = verify_recaptcha(google_recaptcha_secret_key, &parameters.captcha_response, remote_ip).await {
            return application::unauthorized_with_message(&err.to_string(), None, Some(log_data));
        }
    }

    let new_user: NewUser = parameters.into_inner().into();
    match new_user.commit(None, connection.get()) {
        Ok(_) => (),
        Err(e) => match e.error_code {
            ErrorCode::DuplicateKeyError => {
                return application::unprocessable("A user with this email already exists");
            }
            _ => return Err(e.into()),
        },
    };

    if let (Some(first_name), Some(email)) = (new_user.first_name, new_user.email) {
        mailers::user::user_registered(first_name, email, &state.config, connection.get())?;
    }

    Ok(HttpResponse::Created().finish())
}

pub async fn register_with_email_only(
    (state, connection, parameters): (Data<AppState>, Connection, Json<RegisterEmailOnlyRequest>),
) -> Result<HttpResponse, ApiError> {
    if !state.config.email_only_registration_allowed {
        return application::method_not_allowed();
    }

    let mut log_data = HashMap::<&str, Value>::new();
    log_data.insert("email", parameters.email.clone().into());

    let new_user: NewUser = parameters.into_inner().into();
    let conn = connection.get();
    let user = match new_user.commit(None, conn) {
        Ok(user) => user,
        Err(e) => match e.error_code {
            ErrorCode::DuplicateKeyError => {
                return application::unprocessable("A user with this email already exists");
            }
            _ => return Err(e.into()),
        },
    };

    let refresh_token = match user.create_magic_link_token(
        state.service_locator.token_issuer(),
        Duration::minutes(120),
        false,
        conn,
    )? {
        Some(token) => token,
        None => return application::unprocessable("Could not register this user via email only")?,
    };

    mailers::user::user_registered_magic_link(
        &*state.service_locator.create_deep_linker()?,
        &state.config,
        &user.email.unwrap(),
        refresh_token,
        conn,
    )?;

    Ok(HttpResponse::Created().finish())
}

pub async fn register_and_login(
    (http_request, connection, parameters, request_info): (HttpRequest, Connection, Json<RegisterRequest>, RequestInfo),
) -> Result<HttpResponse, ApiError> {
    let state = http_request.state();
    let connection_info = http_request.connection_info();
    let remote_ip = connection_info.remote();
    let mut log_data = HashMap::new();
    log_data.insert("email", parameters.email.clone().into());

    if let Some(ref google_recaptcha_secret_key) = state.config.google_recaptcha_secret_key {
        if let Err(err) = verify_recaptcha(google_recaptcha_secret_key, &parameters.captcha_response, remote_ip).await {
            return application::unauthorized_with_message(&err.to_string(), None, Some(log_data));
        }
    }

    let email = parameters.email.clone();
    let password = parameters.password.clone();
    let new_user: NewUser = parameters.into_inner().into();
    match new_user.commit(None, connection.get()) {
        Ok(_) => (),
        Err(e) => match e.error_code {
            ErrorCode::DuplicateKeyError => {
                return application::unprocessable("A user with this email already exists");
            }
            _ => return Err(e.into()),
        },
    };
    let json = Json(LoginRequest::new(&email, &password));
    let token_response = auth::token((http_request.clone(), connection.clone(), json, request_info)).await?;

    if let (Some(first_name), Some(email)) = (new_user.first_name, new_user.email) {
        mailers::user::user_registered(first_name, email, &state.config, connection.get())?;
    }

    Ok(HttpResponse::Created().json(token_response))
}

fn current_user_from_user(user: &User, connection: &PgConnection) -> Result<CurrentUser, ApiError> {
    let roles_by_organization = user.get_roles_by_organization(connection)?;
    let mut scopes_by_organization = HashMap::new();
    for (organization_id, (roles, additional_scopes)) in &roles_by_organization {
        scopes_by_organization.insert(
            organization_id.clone(),
            scopes::get_scopes(roles.clone(), additional_scopes.clone()),
        );
    }
    let (events_by_organization, readonly_events_by_organization) = user.get_event_ids_by_organization(connection)?;
    let mut event_scopes = HashMap::new();
    for event_user in user.event_users(connection)? {
        event_scopes.insert(event_user.event_id, scopes::get_scopes(vec![event_user.role], None));
    }

    let mut organization_roles = HashMap::new();
    for (key, val) in roles_by_organization.iter() {
        organization_roles.insert(*key, val.0.clone());
    }

    Ok(CurrentUser {
        user: user.clone().for_display()?,
        roles: user.role.clone(),
        scopes: user.get_global_scopes(),
        organization_roles,
        organization_scopes: scopes_by_organization,
        organization_event_ids: events_by_organization,
        organization_readonly_event_ids: readonly_events_by_organization,
        event_scopes,
    })
}

async fn verify_recaptcha(
    google_recaptcha_secret_key: &str,
    captcha_response: &Option<String>,
    remote_ip: Option<&str>,
) -> Result<google_recaptcha::Response, ApiError> {
    match captcha_response {
        Some(ref captcha_response) => {
            let captcha_response =
                google_recaptcha::verify_response(google_recaptcha_secret_key, captcha_response.to_owned(), remote_ip)
                    .await?;
            if !captcha_response.success {
                return Err(ApplicationError::new("Captcha value invalid".to_string()).into());
            }
            Ok(captcha_response)
        }
        None => Err(ApplicationError::new("Captcha required".to_string()).into()),
    }
}

pub async fn delete(
    (conn, path, user): (Connection, Path<PathParameters>, AuthUser),
) -> Result<HttpResponse, ApiError> {
    let conn = conn.get();
    if user.id() != path.id {
        user.requires_scope(Scopes::UserDelete)?
    }

    let target_user = User::find(path.id, conn)?;
    target_user.disable(Some(&user.user), conn)?;
    Ok(HttpResponse::Ok().finish())
}

pub async fn create_marketplace_account(
    (user, state, conn): (AuthUser, Data<AppState>, Connection),
) -> Result<HttpResponse, ApiError> {
    let conn = conn.get();
    let db_user = &user.user;
    if MarketplaceAccount::find_by_user_id(db_user.id, conn)?.len() > 0 {
        return application::unprocessable("User already has a market place account");
    }
    if db_user.email.is_none() {
        return application::unprocessable("Cannot create a market place account for a user with no email");
    }
    let password = OsRng.next_u64().to_string();
    let account = MarketplaceAccount::create(user.id(), db_user.email.clone().unwrap(), password).commit(conn)?;

    let marketplace_api = state.service_locator.create_marketplace_api()?;
    let marketplace_account_id = marketplace_api.link_user(&db_user, &account)?;
    account.update_marketplace_id(marketplace_account_id, conn)?;
    Ok(HttpResponse::Created().finish())
}
