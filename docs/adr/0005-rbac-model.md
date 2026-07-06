# ADR-0005: RBAC model — roles + resource ACLs

- **Status**: Accepted
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
1. Explicit deny rule       (highest priority)
2. Team membership role
3. Resource-level ACL        (e.g., shared with specific user)
4. Default deny             (lowest priority)
```

### Storage

Permissions are stored as:

```
role_permissions:
  role: Role
  action: PermissionAction
  resource_type: ResourceType

resource_acls:
  resource_type: String
  resource_id: Uuid
  subject_type: 'user' | 'team'
  subject_id: Uuid
  permission: PermissionAction
```

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