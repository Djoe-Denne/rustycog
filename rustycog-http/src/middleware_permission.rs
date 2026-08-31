use std::sync::Arc;

use crate::rustycog_permission::{Permission, PermissionChecker, ResourceRef, Subject};
use axum::{
    body::Body,
    extract::State,
    http::{Request, StatusCode},
    middleware::Next,
    response::Response,
};
use tracing::{debug, info};
use uuid::Uuid;

/// Permission middleware settings for a route.
///
/// Constructed by `RouteBuilder::with_permission_on` or
/// `RouteBuilder::with_permission_on_param`.
#[derive(Clone)]
pub struct PermissionGuard {
    pub required: Permission,
    pub object_type: &'static str,
    pub checker: Arc<dyn PermissionChecker>,
    /// Route template used when `resource_param` is set (e.g. `/orgs/{org_id}/members/{user_id}`).
    pub path_template: Option<String>,
    /// Named path parameter to bind as the OpenFGA object id. `None` = deepest UUID.
    pub resource_param: Option<&'static str>,
}

/// Pick the deepest UUID-shaped segment from the request path.
///
/// Routes typically embed resource IDs as path parameters (e.g.
/// `/orgs/{org_id}/projects/{project_id}`); the default permission question
/// is scoped to the most-specific resource, which is the last UUID in the path.
fn extract_deepest_resource_id(path: &str) -> Option<Uuid> {
    path.split('/')
        .rev()
        .filter(|segment| !segment.is_empty())
        .find_map(|s| Uuid::parse_str(s).ok())
}

/// Bind a named `{param}` from `template` onto the request path.
///
/// Extra leading segments on the request (service prefix / nest) are skipped
/// so `/hive/api/orgs/{id}/members/{user}` still matches
/// `/api/orgs/{id}/members/{user}`.
fn extract_named_resource_id(path: &str, template: &str, param: &str) -> Option<Uuid> {
    let needle = format!("{{{param}}}");
    let template_segs: Vec<&str> = template.split('/').filter(|s| !s.is_empty()).collect();
    let path_segs: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    if path_segs.len() < template_segs.len() {
        return None;
    }
    let offset = path_segs.len() - template_segs.len();
    template_segs.iter().enumerate().find_map(|(i, segment)| {
        (*segment == needle).then(|| Uuid::parse_str(path_segs[offset + i]).ok())?
    })
}

fn resolve_resource_id(guard: &PermissionGuard, path: &str) -> Option<Uuid> {
    match (guard.path_template.as_deref(), guard.resource_param) {
        (Some(template), Some(param)) => extract_named_resource_id(path, template, param),
        _ => extract_deepest_resource_id(path),
    }
}

/// Permission-checking middleware. Rejects anonymous callers before touching
/// the checker.
///
/// # Errors
///
/// Returns [`StatusCode::UNAUTHORIZED`] if no user is attached, or
/// [`StatusCode::FORBIDDEN`] if the path has no resource UUID, the checker
/// fails, or the permission is denied.
pub async fn permission_middleware(
    State(guard): State<Arc<PermissionGuard>>,
    req: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let request_path = req.uri().path().to_owned();
    debug!(path = %request_path, "permission_middleware: entering");

    let user_id = req
        .extensions()
        .get::<Uuid>()
        .copied()
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let Some(resource_id) = resolve_resource_id(&guard, &request_path) else {
        debug!(path = %request_path, "permission_middleware: no resource UUID in path -> FORBIDDEN");
        return Err(StatusCode::FORBIDDEN);
    };

    let subject = Subject::new(user_id);
    let resource = ResourceRef::new(guard.object_type, resource_id);

    let allowed = guard
        .checker
        .check(subject, guard.required, resource)
        .await
        .map_err(|e| {
            tracing::warn!(error = %e, "permission_middleware: checker error");
            StatusCode::FORBIDDEN
        })?;

    if !allowed {
        info!(
            user = %user_id,
            permission = %guard.required,
            object_type = guard.object_type,
            object_id = %resource_id,
            "permission_middleware: DENY"
        );
        return Err(StatusCode::FORBIDDEN);
    }

    info!(
        user = %user_id,
        permission = %guard.required,
        object_type = guard.object_type,
        object_id = %resource_id,
        "permission_middleware: ALLOW"
    );
    Ok(next.run(req).await)
}

/// Permission-checking middleware that tolerates anonymous callers.
///
/// If the path has no resource UUID, the middleware passes through without
/// touching the checker — collection-level routes (e.g. `GET /api/projects`)
/// remain anonymously reachable.
///
/// If the path *does* carry a resource UUID, the middleware always consults
/// the centralized `PermissionChecker`:
/// - When a `Subject` (UUID) is attached to the request extensions, the
///   check uses [`Subject::new`] (renders as `user:{uuid}` on the `OpenFGA`
///   wire).
/// - When no subject is attached, the check uses [`Subject::wildcard`]
///   (renders as `user:*`). This honors public-read tuples like
///   `project:{id}#viewer@user:*` written by `sentinel-sync` for public
///   resources, while preserving fail-closed semantics: relations without
///   a wildcard tuple still return `false` and the request 403s.
///
/// Named-param guards (`with_permission_on_param`) fail closed with 403 when
/// the named segment is missing or not a UUID — they never skip the checker.
///
/// # Errors
///
/// Returns [`StatusCode::FORBIDDEN`] if the checker fails or the permission
/// is denied for the resource UUID in the path.
pub async fn optional_permission_middleware(
    State(guard): State<Arc<PermissionGuard>>,
    req: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let request_path = req.uri().path().to_owned();
    debug!(path = %request_path, "optional_permission_middleware: entering");

    let user_id = req.extensions().get::<Uuid>().copied();
    let named = guard.resource_param.is_some();
    let Some(resource_id) = resolve_resource_id(&guard, &request_path) else {
        if named {
            debug!(
                path = %request_path,
                "optional_permission_middleware: named resource UUID missing -> FORBIDDEN"
            );
            return Err(StatusCode::FORBIDDEN);
        }
        return Ok(next.run(req).await);
    };

    let subject = user_id.map_or_else(
        || {
            debug!(
                path = %request_path,
                "optional_permission_middleware: anonymous caller, consulting checker with Subject::wildcard()"
            );
            Subject::wildcard()
        },
        Subject::new,
    );
    let resource = ResourceRef::new(guard.object_type, resource_id);

    let allowed = guard
        .checker
        .check(subject, guard.required, resource)
        .await
        .map_err(|e| {
            tracing::warn!(error = %e, "optional_permission_middleware: checker error");
            StatusCode::FORBIDDEN
        })?;

    if !allowed {
        info!(
            user = %subject,
            permission = %guard.required,
            object_type = guard.object_type,
            object_id = %resource_id,
            "optional_permission_middleware: DENY"
        );
        return Err(StatusCode::FORBIDDEN);
    }

    Ok(next.run(req).await)
}

#[cfg(test)]
mod extract_named_tests {
    use super::*;

    #[test]
    fn named_param_ignores_trailing_uuid() {
        let org = Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap();
        let member = Uuid::parse_str("22222222-2222-2222-2222-222222222222").unwrap();
        let id = extract_named_resource_id(
            &format!("/orgs/{org}/members/{member}"),
            "/orgs/{org_id}/members/{member_id}",
            "org_id",
        );
        assert_eq!(id, Some(org));
    }

    #[test]
    fn named_param_skips_service_prefix() {
        let org = Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap();
        let member = Uuid::parse_str("22222222-2222-2222-2222-222222222222").unwrap();
        let id = extract_named_resource_id(
            &format!("/hive/api/organizations/{org}/members/{member}"),
            "/api/organizations/{organization_id}/members/{user_id}",
            "organization_id",
        );
        assert_eq!(id, Some(org));
    }

    #[test]
    fn named_param_missing_is_none() {
        let org = Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap();
        let id = extract_named_resource_id(
            &format!("/orgs/{org}"),
            "/orgs/{org_id}/members/{member_id}",
            "member_id",
        );
        assert_eq!(id, None);
    }
}
