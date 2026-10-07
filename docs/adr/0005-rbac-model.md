# ADR-0005: RBAC model — roles + resource ACLs

- **Status**: Accepted (amended 2026-10-07 — v1.0 remediation: the model now ships for real)
- **Date**: 2026-07-05
- **Deciders**: Picroom maintainers

## Context

Picroom serves multiple personas (admin, team member, external API consumer).
We need a permission model that:

1. Maps to familiar concepts (admin / manager / uploader / viewer).
2. Supports resource-level sharing (share one image with one user).
3. Is auditable (every decision explained).
4. Is easy to implement correctly.

## Decision

We adopt a **role + resource-ACL** model with deny-overrides-allow semantics.

### Roles

| Role | Default permissions |
|---|---|
| `viewer` | `image.read` (own) |
| `uploader` | `image.read` (own), `image.create` |
| `manager` | all `image.*`, `team.read`, `team.invite` |
| `admin` | everything + `user.*`, `audit.read`, `system.*` |

Custom roles can be defined per team (post-MVP).

### Resources

- `image` is owned by a user; can be `personal` (owner-only) or `team` (shared
  via team membership or explicit ACL).
- `team`, `user`, `audit`, `storage_policy` are system-wide.

### Evaluation order

```
1. Explicit deny rule        (highest priority — beats admin)
2. Resource ownership        (owner_id match)
3. Team membership role      (for the resource's team scope)
4. Resource-level ACL allow  (e.g., shared with a specific user)
5. Global role defaults      (uploader/manager permissions; admin blanket)
6. Default deny              (lowest priority)
```

**An explicit deny overrides the `admin` role.** There is deliberately no
`Admin ⇒ allow` short-circuit in the engine; the admin blanket grant is the
rule-5 tier, below deny. Operators granting a deny to an admin-scoped subject
must expect it to win.

Resources that live **inside** a team (team-shared images) are governed by
membership (rules 3/4): global `uploader`/`manager` roles do not reach into
another team's scope. Only the admin blanket grant crosses team boundaries.
`Team` resources themselves (e.g. member management) are system-wide and
covered by the global-role tier.

### Storage

Permissions are stored as:

```
resource_acls:
  resource_type  String          ('image' | 'team' | 'user' | 'audit' | 'storage_policy' | 'system')
  resource_id    Uuid
  subject_type   'user' | 'team'
  subject_id     Uuid
  permission     'read' | 'create' | 'update' | 'delete' | 'admin'  (admin = wildcard)
  effect         'allow' | 'deny'   (added 2026-10-07, migration 0009)
```

The canonical `Permission` / `ResourceType` / `PermissionAction` vocabulary
lives in `picroom-domain` (with snake_case serde names matching the columns);
`picroom-auth` re-exports it, so the engine and the service layer share one
definition.

### Enforcement (2026-10-07)

Authorization is evaluated in the **service layer**: `UploadService::stage`
enforces `Image/Create` (team-scoped uploads require team membership), and
`DeleteService::delete` enforces `Image/Delete` over the full evaluation
order. HTTP route handlers keep authentication only; `RbacEngine::check`
takes an `Actor` (user id, global roles, team roles), the `Resource`
(including `owner_id` and team scope), and the loaded ACL grants — `Resource`
is engine input, not dead code.

ACL management is exposed for images only (`GET/PUT /api/v1/images/:id/acl`,
`DELETE .../acl/:subject_type/:subject_id`, replace-semantics on `PUT`). The
table and evaluator are resource-agnostic; endpoints for other resource types
are additive when needed.

## Consequences

### Positive

- Familiar mental model (role-based).
- Resource-level ACLs enable "share this one image" without teams.
- Deny rules prevent privilege escalation.
- Auditable: every check writes an entry to the audit log when denied.

### Negative

- Two storage tables (role permissions + ACLs) require careful indexing.
- Custom roles (post-MVP) require a UI for management.

### Neutral

- We do not implement ABAC (attribute-based) or PBAC (policy-based) in v1.
- We do not implement row-level security in PostgreSQL because the access
  patterns are explicit and a single misconfigured RLS policy would be a
  severe bug.

## References

- [NIST RBAC model](https://csrc.nist.gov/projects/role-based-access-control)
- Internal: `docs/spec.md` §10