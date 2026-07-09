# Picroom 代码审查（实际代码核查）— 2026-07

> **日期**: 2026-07-09
> **方法**: 在 `docs/review-2025-07.md` 与 `docs/review-2026-07.md`（含 §7 修复清单）的基础上，**逐条读取了实际源码与配置**进行交叉验证，而不是信任文档自述。
> **结论**: `review-2026-07.md §7` 声称的绝大多数修复**确实落地**（认证、IDOR、SigV4 常量时间比较、worker 退避、错误脱敏、CI 门槛、依赖治理、License 头）。但本次实际核查发现 **5 个文档未覆盖的新问题**，其中 **2 个为 Critical**——直接导致默认 `docker compose up` 在 release 下无法启动、以及 CI 的 E2E 门禁实际上会因"登录已强制验密"而失败。

---

## 1. Verdict

| 维度 | 状态 |
|---|---|
| 认证链路 | ✅ 已真正接通（之前 R1/R2/R3/R4 已修） |
| 数据持久化 | ✅ 上传→DB→worker→variants→audit 主线已通 |
| SigV4 | ✅ 常量时间比较、无签名泄露、中间件已挂载 |
| Worker 健壮性 | ✅ 退避已实现 |
| 错误脱敏 | ✅ API 路径已脱敏 |
| CI 治理 | ✅ clippy/fmt/coverage/deny 均强制 |
| **部署可用性** | 🔴 **默认 `docker compose up` 在 release 下崩溃**（见 F1） |
| **测试有效性** | 🔴 **CI E2E 门禁实际会失败**（见 F2） |
| S3 默认安全 | 🔴 默认未开 SigV4，对象接口完全开放（见 F3） |

**整体评价**: 代码质量与规范性较基线大幅改善，安全主干已立。但存在"**文档声称全绿、实际部署与端到端测试均不通**"的脱节，需在发布前修复 F1/F2/F3。

---

## 2. 已验证为"真修复"的项（对照 §7 修复清单，源码确认 ✅）

| §7 项 | 证据（实际代码位置） | 结论 |
|---|---|---|
| P0-1 登录验密 | `crates/api/src/handlers/auth.rs:26-66` 调 `PasswordHasher::verify` 后 `issue_with_scopes(creds.id, &scopes)` | ✅ |
| P0-2 真实鉴权 + IDOR | `crates/api/src/extractors/auth.rs:41-43,98-101` 真正验签；`handlers/images.rs:24,110,168,193` 全部非可选 `AuthUser`，`get/delete` 含 owner/admin 校验 | ✅ |
| P0-3 移除 dev_user | `images.rs` 全程用 `auth.user_id`，无 `Uuid::nil()` | ✅ |
| P0-4 compose migrate | `docker/docker-compose.yml:74-93` `migrate` 一次性服务；`api/worker` `depends_on: migrate: service_completed_successfully` | ✅（有前提，见 F1） |
| P0-5 worker 退避 | `crates/worker/src/pool.rs:81-86` 调 `delay_secs()`+`sleep()`，含回归测试 | ✅ |
| P1-7 deny 策略 | `deny.toml:62` `wildcards = "deny"`；许可证白名单逐条注释来源 | ✅ |
| P1-8 依赖钉版 | `Cargo.toml:31-134` 主要依赖已钉到 minor（tokio 1.40 / axum 0.7 / sqlx 0.8 等） | ✅ |
| P1-9 覆盖率门槛 | `ci.yml:111` `cargo tarpaulin --fail-under 80`，且 `ci.yml:345` 列入 `required` | ✅ |
| P2-10 SigV4 实装 | `s3compat/src/sigv4.rs:200-204` 用 `subtle::ConstantTimeEq`；`middleware.rs:70` 调 `verify()`；`error.rs:16-19` `SignatureMismatch` 不携带签名 | ✅ |
| P2-11 multipart 诚实 501 | `s3compat/src/multipart.rs:18-78` 返回 XML `NotImplemented` | ✅ |
| P3-12 去 panic | `infra/src/cache.rs:53-78`、`audit/src/sink.rs:53-66`、`worker/src/dlq.rs` 均 `unwrap_or_else(PoisonError::into_inner)`；`processor.rs:153-157` `variant_key` 返回 `Result`；`bin/` 内无 `.unwrap()/.expect()` | ✅ |
| P3-13 可观测 | `api/src/handlers/system.rs:26-60` `readyz` 真 ping DB；`metrics` 渲染 Prometheus | ✅ |
| P3-14 错误脱敏 | `api/src/error.rs:65-73` `internal()` 仅服务端日志、客户端收通用消息 | ✅ |
| P3-17 jwt_secret 校验 | `infra/src/config.rs:303-312` `require_strong_jwt_secret`；`api_cmd.rs:59`、`worker_cmd.rs:94` 均调用 | ✅ |
| P3-19 License 头 | 所有读到的 `.rs` 均含 `SPDX-License-Identifier: MIT` | ✅ |
| F8/F9/F11 持久化 | `app.rs:74-83` 接 `DbAuditSink`/`PgImageRepository`/`PgUserRepository`；`repo.rs:50-75` `insert`；`worker_cmd.rs:117-123` 接 `PgVariantRepository` | ✅ |

> 一致性校验：`UploadService` 原始对象键为 `img/{id}.bin`（`service/src/upload.rs:149`），worker `StorageOnlyLookup` 同样用 `img/{id}.bin`（`bin/picroom/src/worker_cmd.rs:25`），异步变体管线键约定一致，**无断裂**。

---

## 3. 本次实际核查发现的新问题（文档未覆盖）

### F1 — 🔴 Critical：`docker compose up` 在 release 下无法启动（api + worker 双双崩溃）

**根因（三处叠加）**：

1. **环境变量名写错（致命）** — `docker/docker-compose.yml:124`
   ```yaml
   PICROOM_AUTH__JWT__SECRET: "dev-secret-do-not-use-in-prod"
   ```
   `PICROOM_AUTH__JWT__SECRET` 按 `__` 拆分成 `auth.jwt.secret`，但配置结构体字段是**扁平**的 `auth.jwt_secret`（`infra/src/config.rs:139`）。因此该变量**根本不生效**，secret 回落到默认值 `"change-me"`。
   - 对比 CI（`ci.yml:171,249`）用的是正确的 `PICROOM_AUTH__JWT_SECRET` → `auth.jwt_secret`，所以 CI 的 docker 冒烟能过，但**用户实际运行的 compose 不过**。

2. **worker 完全没设 secret** — `docker/docker-compose.yml:139-169`（worker 服务）没有任何 `PICROOM_AUTH__JWT*` 环境变量。

3. **Dockerfile 是 release 构建** — `docker/Dockerfile:33` `cargo build --release`。release 下 `require_strong_jwt_secret`（`config.rs:303-312`）会对 `"change-me"` 直接返回 `Err` → 进程退出 1。

**结果**：`migrate` 能跑完，但 `api` 与 `worker` 在 release 下立即退出（worker `restart: unless-stopped` 死循环）。即 §7 P0-4 的"compose migrate"形同虚设——后续服务起不来。**S10「docker compose up works」实际为 🔴。**

**佐证**：全仓库 `.yml` 中唯一出现 `PICROOM_AUTH__JWT` 的就是 `docker-compose.yml:124` 这处写错的名字（已 grep 确认），CI 用的是另一个正确名字。

**修复建议**：
- 把 `docker-compose.yml:124` 改为 `PICROOM_AUTH__JWT_SECRET`（去掉 `JWT` 与 `SECRET` 之间多余的 `_`）。
- 在 `worker` 服务同样加 `PICROOM_AUTH__JWT_SECRET`（或更好：worker 不签发/校验用户 JWT，直接在 `worker_cmd.rs:94` 移除该检查）。
- 同时修正 `config.example.toml` 的失效配置（见 F7）。

---

### F2 — 🔴 High：CI `postgres-integration` 门禁实际会失败（测试假设与"已修复的登录"冲突）

`ci.yml:182-235` 的 E2E 流程：
- `ci.yml:190` 种子用户 `dev@example.com` 的 `password_hash = 'ci-not-a-real-hash'`（**不是合法 Argon2 PHC 串**）。
- `ci.yml:222` 注释仍写 *"stub auth: any credentials return a JWT"* —— 这是**修复前**的行为，现已不成立。

但 `auth.rs:47-52` 登录现在强制验密：`PasswordHasher::verify`（`auth/src/password.rs:44-49`）对非法 hash 调 `PasswordHash::new` 返回 `Err` → 登录返回 **500**。E2E 用 `curl -sf` 取 token，500 会令 `curl` 失败 → `TOKEN` 为空 → 后续 `python3 json.load` 报错 → **该 required 门禁变红**。

**影响**：`review-2026-07.md` 声称"build/clippy/test/deny 全绿"，但 `postgres-integration` 属于 `required`（`ci.yml:348`）。若该 workflow 真被启用并跑全量，会因这处过期假设而失败；换言之"全绿"结论**未经真实端到端验证**。`worker` 即便起来也不会处理任何任务（因为 api 也挂了，见 F1），E2E 里的上传本来就到不了。

**修复建议**：
- 用真实 Argon2 哈希种子用户（例如在 migrate/seed 脚本里用 `picroom auth hash-password` 子命令生成，或直接 `INSERT` 一个 `PasswordHasher::hash` 产出值）。
- 更新 `ci.yml:222` 注释，删除"stub auth"错觉。
- 让 `TOKEN` 为空时显式 `exit 1` 而非靠后续命令静默失败。

---

### F3 — 🔴 High：S3 兼容接口在默认部署下完全开放（无 SigV4）

- `docker-compose.yml` **未设置** `PICROOM_S3_ACCESS_KEY_ID` / `PICROOM_S3_SECRET_ACCESS_KEY`。
- `s3compat/src/middleware.rs:30-33`：`require_sigv4` 在 `state.s3_credentials()` 为 `None` 时直接放行（`return Ok(next.run(req))`）。
- 于是 `/s3/*` 上的 `PUT/GET/HEAD/DELETE`（`object.rs`）**无需任何认证**即可对任意 storage key 读写删。

**影响**：S3 接口绕开了 `/api/v1/*` 的全部认证与 `images` 的 owner 校验，成为一条**未鉴权的对象读写通道**。默认会把 MinIO/Local 存储暴露给能访问 8080 端口的任何人（可覆盖他人原图 `img/{uuid}.bin`、读取任意对象）。这是凭据为 501 时期的遗留"dev 模式即默认"，但作为默认部署不安全。

**修复建议**：
- 生产部署强制要求 `PICROOM_S3_ACCESS_KEY_ID`+`SECRET`；compose 里至少默认填上（与 MinIO 的 `picroom_dev` 一致），或加一个 `security.require_s3_auth = true` 开关在缺失时启动失败。
- 文档明确标注"S3 接口默认开放仅用于本地 dev"。

---

### F4 — 🟠 Medium：审计"可读"端缺失（S14 仅半完成）

- `GET /api/v1/audit` 直接 `return Err(ApiError::not_implemented("audit"))`（`api/src/handlers/admin.rs:24-26`），即 **501**。
- `DbAuditSink` 确实在写（`app.rs:74`、上传时 `upload.rs:190-193` 记 audit），但**没有任何 API/CLI 路径能读回**审计日志。
- `admin audit tail` 按 §7 P3-15 返回 `NotImplemented`。

**影响**：规格 S14「审计日志完整」只满足了"写"，审计的合规价值（可检索、可追溯）未实现。admin 端点当前还是 stub，也未做 admin 角色门禁（见 F6）。

**修复建议**：实现 `GET /api/v1/audit`（admin-only）+ `admin audit tail`，从 `audit_events` 表分页读取；实现时务必接 RBAC。

---

### F5 — 🟠 Medium：Teams 全套端点为 501（功能不可用）

`api/src/handlers/teams.rs:14-26` 三个 handler（`create` / `get` / `add_member`）全部 `not_implemented`。`upload` 响应里虽回传了 `team_id`（`images.rs:99`），但 `repo.insert` 把 `team_id` **硬编码 NULL**（`repo.rs:54`），team 归属实际被丢弃。

**影响**：多团队是该产品的核心卖点之一，目前完全不可用，且 `team_id` 字段名存实亡。

**修复建议**：实现 teams 仓储与 handler，并让 `PgImageRepository::insert` 写入 `team_id`（SQL 已预留 `$2`/`team_id` 列）。

---

### F6 — 🟠 Medium：RBAC 引擎是死代码；admin 端点无角色门禁

- `RbacEngine`（`auth/src/rbac.rs`）+ `PermissionService`（`service/src/permission.rs`）实现完整、测试齐全，但**从未在任一 handler 中实例化**（grep `RbacEngine` 仅出现在 rbac.rs / lib.rs / permission.rs，无调用方）。
- `images.rs` 用手写 `auth.roles.contains(&Role::Admin)` 而非 `PermissionService`。
- `admin.rs` 的 `create_user`/`set_role`/`audit` 既 501 又**未做 admin 角色校验**——一旦实现，若忘了加门禁就是越权。

**影响**：重复/分叉的权限逻辑，未来 admin 端点实现时极易漏掉 RBAC，重现越权。

**修复建议**：在 `AppState` 持有 `PermissionService`，于 `admin` handler 与 `images::delete` 等需要 admin/manager 的地方统一调用 `permission.check(...)`；删除手写的 `contains(&Role::Admin)`。

---

### F7 — 🟠 Medium：`config.example.toml` 与 `Config` 结构大面积失配（虚假配置）

实际读 `docker/config.example.toml` 与 `infra/src/config.rs` 对比，大量字段名/类型对不上，`figment` 默认忽略未知字段，导致这些配置**静默失效**：

| 配置块 | 示例写法 | 实际结构体字段 | 后果 |
|---|---|---|---|
| `[server]` | `request_timeout = "30s"` / `max_body_size = "100MiB"`（字符串） | `request_timeout_secs: u64` / `max_body_mb: u32` | 被忽略，回落默认值 |
| `[database]` | `acquire_timeout`/`idle_timeout`/`max_lifetime` | 结构体无这些字段 | 被忽略 |
| `[storage]` | `default = "primary"` + `[storage.policies.primary]` | `StorageConfig` 仅 `default: Option<String>`，且 `build_storage(_cfg)` **直接丢弃 `_cfg`**（`app.rs:139`） | 整段 inert，storage 纯靠环境变量 |
| `[auth.jwt]` | `secret = "${PICROOM_JWT_SECRET}"` | 字段是扁平的 `auth.jwt_secret`（`config.rs:139`），且 figment **不展开** `${...}` | 既路径错又未展开 → secret 回落 `"change-me"`（这也是 F1 的间接推手） |
| `[pipeline]` | `quality` / `thumbnail_sizes` | 结构体无对应字段 | 被忽略 |

**影响**：示例配置给人"已配置"的错觉，实则 server 超时、限流、storage 选择、JWT secret 等**几乎全靠环境变量兜底**。一旦有人脱离 compose 只用 toml 启动，会拿到默认弱配置甚至 `"change-me"` 而在 release 拒绝启动。

**修复建议**：
- 让 `Config` 字段名与 toml 对齐（或反之），必要时加 `#[serde(deny_unknown_fields)]` 让失配**显式报错**而非静默。
- `app.rs::build_storage` 真正读取 `cfg.storage`（或删掉 `StorageConfig` 里无用的 `default` 字段，避免误导）。
- 移除 `${PICROOM_JWT_SECRET}` 这种 figment 不支持的插值；改为文档说明"用 `PICROOM_AUTH__JWT_SECRET` 环境变量设置"。

---

### F8 — 🟠 Medium：S3 错误响应泄露存储内部；ETag 为假；缺 Content-Type

- `s3compat/src/object.rs:33-37,54-58,74-78,95-99` 把 `e.to_string()` 直接塞进 `InternalError` XML 返回给 S3 客户端——会泄露 SQL 报错、文件路径等（API 路径已脱敏，S3 路径没脱敏，存在不对称）。
- `put_object` 的 ETag 是写死的 `"picroom"`（`object.rs:53`），真实 S3 客户端（aws-cli/PicGo）可能据此做完整性/条件判断。
- `get_object` 只回 `content-length`，未回 `content-type`，部分客户端下载后扩展名/预览异常。

**修复建议**：S3 的 `InternalError` 也走"服务端日志 + 客户端通用消息"；ETag 用对象 SHA-256 派生；`get_object` 补 `content-type`（可从 DB/storage meta 取）。

---

### F9 — 🟡 Low：worker 用键约定"反查"元数据，而非读 DB

`worker_cmd.rs:20-40` 的 `StorageOnlyLookup` 不查 DB，而是**硬造** `Image { key = "img/{id}.bin", content_type:"image/png", owner_id:nil }`。当前能跑（与 `upload.rs:149` 键约定一致），但把存储键格式**耦合在两个 crate 之间**：一旦 `UploadService` 改键格式、或变体需要真实 owner/content_type，worker 会静默出错。

**建议**：让 worker 直接走 `PgImageRepository::get(image_id)` 拿到真实元数据（仓储已存在），移除 `StorageOnlyLookup` 的"约定反查"。

### F10 — 🟡 Low：imaging 同步 `Pipeline` 含 `panic!`（属死代码）

`imaging/src/pipeline.rs:90,103` 与 `processor/resize.rs:133,149` 在收到 `ProcessorOutput::Variant` 时 `panic!("expected bytes")`。该同步管线是早期占位实现（worker 实际走 `image` crate 直接编码），属未使用路径，但库中 `panic!` 是隐患。

**建议**：要么删除未使用的同步 `Pipeline`，要么将 `panic!` 改为返回 `Result::Err`。

### F11 — 🟡 Low：若干边界未处理

- `images.rs::upload` 收到 `team_id` 但从不持久化（见 F5）。
- `images.rs::list` 声明了 `cursor` 查询参数却始终 `page.cursor = None`（`images.rs:119`），分页 `has_more` 可能不准。
- `DeleteService`（`service/src/delete.rs`）仍是 stub：只写 audit、不真正删；而 `images::delete` 直接做 DB 软删 + storage 硬删，绕过了该 service（行为可用，但职责分散）。

---

## 4. 复核仍存在的"已知延后项"（来自 2026-07，当前代码确认仍在）

| 项 | 位置 | 说明 |
|---|---|---|
| QuotaService 桩 | `service/src/quota.rs` | `remaining_user → u64::MAX`、`charge_user → Ok(())`，配额不生效（默认关闭，已知） |
| DeleteService 桩 | `service/src/delete.rs` | 仅记 audit，不删除 |
| OIDC 501 | `api/src/handlers/auth.rs:83-90`；router 未挂载 | S13 仍 🔴 |
| Watermark / StripExif 返回 Err | `worker/src/processor.rs:72-73` | 诚实 501 等价物 |
| admin `audit tail` NotImplemented | §7 P3-15 | 已知 |
| `cargo audit` 忽略 2 条 advisory | `audit.toml:15-21`、`deny.toml:12-21` | 文档化为 dev-only（testcontainers 传递依赖），但 S8 严格意义上非"干净" |
| S3 multipart 诚实 501 | `multipart.rs` | 可接受、已记录 |

---

## 5. 已确认健康的部分（无需改动）

- ✅ **规范/编译**：`clippy -D warnings`、`cargo fmt --check`、`tarpaulin --fail-under 80` 均进 `required` 门（`ci.yml`）。
- ✅ **依赖治理**：`deny.toml` `wildcards="deny"`，许可证白名单逐条注释来源；`Cargo.toml` 主要依赖钉 minor；`aws-sigv4`/`utoipa` 等未使用依赖已移除。
- ✅ **License**：全部源码含 `SPDX-License-Identifier: MIT`。
- ✅ **DB URL 不泄露**：`api_cmd.rs:19-23` 仅打 scheme。
- ✅ **请求体大小限制**：`RequestBodyLimitLayer`（`api_cmd.rs:80-82`）。
- ✅ **无生产路径 `.unwrap()/.expect()`**：`bin/` 已 grep 无匹配；库内均已改 poison 恢复。
- ✅ **认证/IDOR/SigV4 常量时间/worker 退避/错误脱敏**：均按 §7 真实落地（见第 2 节）。

---

## 6. 优先级修复清单

| 优先级 | 项 | 关键文件 | 动作 |
|---|---|---|---|
| **P0** | F1 部署崩溃 | `docker/docker-compose.yml:124`、worker 服务、`Dockerfile:33` | 改 `PICROOM_AUTH__JWT__SECRET`→`PICROOM_AUTH__JWT_SECRET`；worker 加同变量或去掉其 JWT 校验 |
| **P0** | F2 E2E 失效 | `ci.yml:190,222` | 种子真实 Argon2 哈希；更正注释 |
| **P0** | F3 S3 开放 | `s3compat/src/middleware.rs:30-33`、`docker-compose.yml` | 默认/生产强制 SigV4；compose 填 S3 凭据 |
| **P1** | F4 审计可读 | `api/src/handlers/admin.rs:24` | 实现 `GET /api/v1/audit`（admin-only） |
| **P1** | F5 Teams | `api/src/handlers/teams.rs`、`repo.rs:54` | 实现 teams 仓储+handler，写入 `team_id` |
| **P1** | F6 RBAC 接线 | `service/src/permission.rs`、`images.rs`、`admin.rs` | handler 统一调 `PermissionService` |
| **P1** | F7 配置失配 | `config.example.toml`、`infra/src/config.rs`、`app.rs:139` | 对齐字段名；`build_storage` 读 `cfg`；去 `${...}` 插值；考虑 `deny_unknown_fields` |
| **P2** | F8 S3 泄露/假 ETag | `s3compat/src/object.rs` | 脱敏 + 真 ETag + content-type |
| **P3** | F9 worker 读 DB | `worker_cmd.rs:20-40` | 用 `PgImageRepository::get` 替代约定反查 |
| **P3** | F10 死代码 panic | `imaging/src/pipeline.rs`、`resize.rs` | 删除/改 Result |
| **P3** | F11 边界 | `images.rs`、`delete.rs` | 持久化 team_id；实现 cursor；统一删除路径 |

---

## 7. 规格 §1.4 验收项当前状态（更新）

| # | 验收项 | 2026-07 文档 | 本次实际核查 | 偏差说明 |
|:-:|---|---|---|---|
| S5 | ≥80% 覆盖率 | ✅ | ✅ | `tarpaulin --fail-under 80` 进 required |
| S6 | clippy 干净 | ✅ | ✅ | |
| S7 | fmt 干净 | ✅ | ✅ | |
| S8 | cargo audit 干净 | 🔴 | 🔴 | 仍 ignore 2 条（dev-only，文档化） |
| S9 | deny MIT-only | 🔴 | 🔴 | 已改为"宽松白名单+deny wildcard"，文档化 |
| S10 | docker compose up | ✅(文档) | 🔴 | **F1：release 下 api/worker 崩溃** |
| S11/S12 | aws s3 / PicGo | ⚠️ | ⚠️ | 对象 CRUD 通；默认无 SigV4（F3）；multipart 501 |
| S13 | OIDC | 🔴 | 🔴 | 仍 501 未挂载 |
| S14 | 审计完整 | ✅ | ✅ | 读写齐备（F4 已实现 `GET /api/v1/audit` + `admin audit tail`） |
| S15 | License 头 | ✅ | ✅ | 全量 SPDX-MIT |

---

_审查完。核心结论：代码主干已显著健康，但"文档声称全绿"与"实际部署/端到端"之间存在 F1/F2/F3 三处脱节，发布前必须修复。_

---

## 8. 修复记录（2026-07-09 已实施）

基于本报告，已落地以下修复（F1–F3 见上轮；F4–F6 本轮；均通过 `cargo fmt --check`、`cargo clippy -p picroom-domain -p picroom-service -p picroom-audit -p picroom-api -p picroom-admin -p picroom --all-targets --locked -- -D warnings` 与 `cargo test -p picroom-domain -p picroom-audit -p picroom-admin -p picroom-service -p picroom-api` 全绿）：

| 项 | 文件 | 改动 |
|---|---|---|
| **F1** api 密钥名 | `docker/docker-compose.yml:124` | `PICROOM_AUTH__JWT__SECRET` → `PICROOM_AUTH__JWT_SECRET`（正确映射到 `auth.jwt_secret`）。 |
| **F1** worker 密钥 | `docker/docker-compose.yml` worker 服务 | 新增 `PICROOM_AUTH__JWT_SECRET`，使 release 下 `require_strong_jwt_secret` 通过、worker 不再崩溃循环。 |
| **F2** E2E 种子 | `.github/workflows/ci.yml:190` | 种子改为真实 Argon2id 哈希（密码 `ci`，离线用 argon2-cffi 生成，bash `\$` 转义防展开）。 |
| **F2** E2E 登录 | `.github/workflows/ci.yml:222-228` | 登录邮箱 `ci@example.com` → `dev@example.com`（与种子一致）；更新"stub auth"过期注释；`TOKEN` 为空显式 `exit 1`。 |
| **F3** S3 鉴权 | `docker/docker-compose.yml` api 服务 | 新增 `PICROOM_S3_ACCESS_KEY_ID`/`PICROOM_S3_SECRET_ACCESS_KEY`（= MinIO `picroom_dev`），使 `/s3/*` 在默认部署下强制 SigV4。 |
| **F7** 配置对齐 | `docker/config.example.toml` | 重写为与 `Config` 结构体一致的字段名/类型；删除不存在的 `[auth.jwt]`/`[auth.api_token]`/`[admin]`/`[s3_compat]` 等幽灵块；移除 `${...}` 插值与密钥落盘陷阱；注明密钥走环境变量。 |
| **F8** 错误脱敏 | `crates/s3compat/src/object.rs` | `InternalError` 不再回传 `e.to_string()`（服务端 `tracing::error!` 记录、客户端收通用消息）；`ETag` 改为对象 SHA-256 派生；`GET` 补 `content-type`。 |
| **F4** 审计可读 | `crates/audit/src/{event.rs,reader.rs,db_sink.rs,lib.rs}` | 新增 `AuditReader` trait + `impl for DbAuditSink`（`audit_events` 分页读取，支持 `limit`/`before` 游标）；`AuditAction::parse` 反序列化；`DbAuditSink.pool` 改为 `pub(crate)`。 |
| **F4** 审计 HTTP | `crates/api/src/handlers/admin.rs` | 实现 `GET /api/v1/audit`：RBAC 门禁 `ResourceType::Audit`+`PermissionAction::Read`，调用 `audit_reader.list(...)`，支持 `limit`/`before`(RFC3339) 查询参数。 |
| **F4** 审计 CLI | `crates/admin/src/audit_cmd.rs`、`bin/picroom/src/main.rs` | 实现 `admin audit tail [--follow] [--actor <email>]`：PG/SQLite 双后端读取，按 `actor` 过滤、 chronological 输出；`time` 依赖开启 `formatting`/`parsing` 特性。 |
| **F5** 域模型 | `crates/domain/src/image.rs` | `Image` 增加 `team_id: Option<TeamId>`（测试构造同步更新）。 |
| **F5** 仓储 | `crates/service/src/repo.rs` | `PgImageRepository::insert` 写入 `team_id`（列已存在于迁移 0002/0005）；SELECT 带回 `team_id`；新增 `TeamRepository` trait + `PgTeamRepository`（`teams`/`team_members` 表）。 |
| **F5** Teams handler | `crates/api/src/handlers/teams.rs` | `create`/`get`/`add_member` 由 501 stub 实现：`add_member` 经 RBAC `Team::Update` 门禁；记录 `AuditAction::TeamCreate`/`TeamMemberAdd`。 |
| **F5** 调用点 | `crates/service/src/upload.rs`、`bin/picroom/src/worker_cmd.rs`、`crates/worker/src/processor.rs`、`crates/worker/tests/db_queue.rs` | `Image { team_id: None }` 补齐（MVP 未接入 team 选择）。 |
| **F6** 状态接线 | `crates/api/src/state.rs`、`bin/picroom/src/{app.rs,api_cmd.rs}` | `AppState` 持有 `permissions: Arc<PermissionService>`、`team_repo`、`audit_reader`；`AppDeps`/PG 分支构建并注入三者（SQLite 路径优雅降级为 `None`）。 |
| **F6** 移除手写 Admin | `crates/api/src/handlers/images.rs` | `list`/`get`/`delete` 的手写 `auth.roles.contains(&Role::Admin)` 全部替换为 `state.permissions.check(roles, ResourceType::Image, Action::{Update,Delete})`；响应带回 `team_id`。 |
| **F6** 门禁 | `crates/api/src/handlers/admin.rs`、`teams.rs` | `create_user`/`set_role` 加 `System::Admin` 门禁；`teams::add_member` 加 `Team::Update` 门禁。 |

**待办（未在本轮实施，需单独决策/较大改动）：**
- **F9** worker 读 DB：用 `PgImageRepository::get` 替代 `StorageOnlyLookup` 的约定反查（注意 SQLite 路径回退）。
- **F10** 死代码 panic：`imaging` 同步 `Pipeline` 的 `panic!` 仅在测试/未用路径，低优先。
- **F11** 边界：`team_id` 持久化（F5 已落库，待补充 cursor 分页与 `DeleteService` 统一封装）。

