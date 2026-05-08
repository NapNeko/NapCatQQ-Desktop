# NapCat Daemon 安全实施文档

## 已完成的安全功能

### 1. 安全核心库 (`pkg/security/`)

| 模块 | 功能 | 文件 |
|------|------|------|
| Challenge | 挑战-响应认证，HMAC-SHA256 | `challenge.go` (145行) |
| JWT | JWT 会话管理，权限声明 | `jwt.go` (285行) |
| RateLimit | 速率限制，IP/连接限制 | `ratelimit.go` (250行) |
| Audit | 安全审计日志 | `audit.go` (350行) |
| Validation | 输入验证，路径安全 | `validation.go` (180行) |

**总计: 1210行 Go 安全代码**

### 2. 安全服务器 (`internal/server/secure_server.go`)

集成所有安全功能的强化服务器 (500行)：

- TLS 1.3 强制
- 挑战-响应认证
- JWT 会话管理
- 权限分级控制
- 速率限制
- 审计日志
- 输入验证

### 3. Python 客户端安全对应

需要同步实现：
- HMAC-SHA256 挑战签名
- JWT Token 管理
- 证书固定 (可选)

## 安全功能详情

### 挑战-响应认证流程

```
1. Desktop          Daemon
   │ ──请求挑战────► │
   │                 │ 生成随机挑战 (32字节)
   │ ◄──返回挑战─── │
   │                 │
   │ 计算: signature = HMAC(Token, challenge)
   │ ──签名────────► │
   │                 │ 验证签名
   │ ◄──返回 JWT─── │ 生成短期 JWT (15分钟)
```

**安全特性**:
- 挑战单次使用
- 5分钟过期
- 常量时间比较 (防时序攻击)
- 暴力破解保护

### JWT 会话设计

```go
Claims:
  - Subject:    session_id
  - IssuedAt:   签发时间
  - ExpiresAt:  过期时间 (15分钟)
  - SessionID:  会话标识
  - ClientIP:   绑定 IP
  - Permissions: ["status:read", "status:write", ...]
```

### 权限分级

```go
PermStatusRead   = "status:read"    // 查看状态
PermStatusWrite  = "status:write"   // 启动/停止
PermConfigRead   = "config:read"    // 读取配置
PermConfigWrite  = "config:write"   // 修改配置
PermFileRead     = "file:read"      // 下载文件
PermFileWrite    = "file:write"     // 上传文件
PermAdmin        = "admin"          // 所有权限
```

### 审计日志事件

```
auth.challenge.requested
auth.challenge.failed_expired
auth.success
auth.failure.invalid_signature
auth.failure.token_mismatch
session.created
session.expired
permission.denied
rate_limit.hit
security.path_traversal_attempt
command.blocked
```

### 速率限制策略

| 端点 | 限制 |
|------|------|
| 认证 | 5次/15分钟，失败超5次封禁15分钟 |
| 一般请求 | 10次/秒，突发20 |
| 连接数 | 每IP最多5个并发 |

### 输入验证

```go
ValidatePath():      // 防止目录遍历
  - 禁止 .. 序列
  - 禁止系统目录 (/etc, /usr, ...)
  - 必须在允许的基础目录下

ValidateWorkDir():   // 工作目录安全
  - 禁止父目录引用
  - 检查绝对路径白名单

ValidateFileExtension():
  - 只允许: .json, .yaml, .conf, .txt, .log, .sh

MaxFileSize(): 10MB
```

## 使用方式

### 启动安全服务器

```go
// 创建服务器 (强制 TLS)
server, err := server.NewSecure(config, true)
if err != nil {
    log.Fatal(err)
}

// 配置 TLS
er := server.SetTLSConfig("/path/to/cert.pem", "/path/to/key.pem")
if err != nil {
    log.Fatal(err)
}

// 启动
if err := server.Start(); err != nil {
    log.Fatal(err)
}
```

### 客户端认证 (Python 需实现)

```python
import hmac
import hashlib

# 1. 获取挑战
challenge = request_challenge()

# 2. 计算 HMAC
signature = hmac.new(
    token.encode(),
    challenge.encode(),
    hashlib.sha256
).hexdigest()

# 3. 发送认证
jwt_token = authenticate(challenge, signature)

# 4. 后续请求携带 JWT
response = call_method("napcat.start", {"token": jwt_token})
```

## 安全配置检查清单

### 生产环境必须

- [ ] 强制 TLS 1.3 (`requireTLS: true`)
- [ ] 使用受信任证书 (Let's Encrypt/商业证书)
- [ ] 禁用 ws://，只允许 wss://
- [ ] 强 Token (32+ 字节随机)
- [ ] 审计日志开启
- [ ] 速率限制启用

### 建议配置

- [ ] 证书固定 (防中间人)
- [ ] mTLS (双向证书)
- [ ] 外部审计日志收集
- [ ] 异常检测告警

## 安全测试建议

```bash
# 中间人测试
mitmproxy --mode transparent

# 暴力破解测试
hydra -l admin -P passwords.txt wss://target/ws

# 重放攻击测试
# 捕获认证包，重复发送

# 路径遍历测试
# 尝试: ../../../etc/passwd

# 命令注入测试
# 尝试: ; cat /etc/passwd
```

## 下一步实施

1. **Python 客户端安全库**
   - HMAC 计算
   - JWT 解析/刷新
   - Token 安全存储

2. **证书管理**
   - 自签名证书生成脚本
   - Let's Encrypt 自动续期

3. **UI 安全提示**
   - TLS 状态指示
   - 认证失败警告
   - 安全事件通知
