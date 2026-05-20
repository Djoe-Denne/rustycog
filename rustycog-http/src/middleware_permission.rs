use std::sync::Arc;

use axum::{
    body::Body,
    extract::State,
    http::{Request, StatusCode},
    middleware::Next,
    response::Response,
};
use rustycog_permission::{Permission, PermissionChecker, ResourceRef, Subject};
use tracing::{debug, info};
use uuid::Uuid;

/// Permission middleware settings for a route.
///
/// Constructed by `RouteBuilder::with_permission_on`. The middleware takes the
/// deepest UUID path segment of the request, builds a `ResourceRef` of
/// `object_type`, and asks the shared `PermissionChecker` whether the caller
/// is allowed to perform `required`.
#[derive(Clone)]
pub struct PermissionGuard {
    pub required: Permission,
    pub object_type: &'static str,
    pub checker: Arc<dyn PermissionChecker>,
}

/// Pick the deepest UUID-shaped segment from the request path.
///
/// Routes typically embed resource IDs as path parameters (e.g.
/// `/orgs/{org_id}/projects/{project_id}`); the permission question we want to
/// answer is always scoped to the most-specific resource, which is the last
/// UUID in the path.
fn extract_deepest_resource_id(path: &str) -> Option<Uuid> {
    path.split('/')
        .rev()
        .filter(|segment| !segment.is_empty())
        .find_map(|s| Uuid::parse_str(s).ok())
}

/// Permission-checking middleware. Rejects anonymous callers before touching
/// the checker.
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

    let Some(resource_id) = extract_deepest_resource_id(&request_path) else {
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
pub async fn optional_permission_middleware(
    State(guard): State<Arc<PermissionGuard>>,
    req: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let request_path = req.uri().path().to_owned();
    debug!(path = %request_path, "optional_permission_middleware: entering");

    let user_id = req.extensions().get::<Uuid>().copied();
    let Some(resource_id) = extract_deepest_resource_id(&request_path) else {
        return Ok(next.run(req).await);
    };

    let subject = if let Some(uid) = user_id {
        Subject::new(uid)
    } else {
        debug!(
            path = %request_path,
            "optional_permission_middleware: anonymous caller, consulting checker with Subject::wildcard()"
        );
        Subject::wildcard()
    };
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
