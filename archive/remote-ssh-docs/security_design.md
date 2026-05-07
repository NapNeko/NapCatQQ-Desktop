# NapCat Daemon 安全设计文档

## 1. 安全威胁模型

### 1.1 攻击面分析

```
┌─────────────────────────────────────────────────────────────┐
│  攻击者                                                      │
│     │                                                        │
│     ├──► 网络窃听 ──► 获取 Token/通信内容                    │
│     ├──► 中间人攻击 ──► 篡改指令                             │
│     ├──► 重放攻击 ──► 重复执行敏感操作                       │
│     ├──► 暴力破解 ──► 猜测 Token                             │
│     ├──► 拒绝服务 ──► 耗尽连接/内存                          │
│     └──► 权限提升 ──► 通过 Daemon 执行任意命令               │
└─────────────────────────────────────────────────────────────┘
```

### 1.2 当前弱点

| 层级 | 当前状态 | 风险等级 |
|------|----------|----------|
| 传输层 | WebSocket (ws://)，明文传输 | 🔴 高危 |
| 认证 | 固定 Token，无过期 | 🟠 中危 |
| 会话 | 无会话管理，无绑定 | 🟠 中危 |
| 授权 | 无权限分级 | 🟡 低危 |
| 审计 | 无安全审计日志 | 🟡 低危 |
| 输入 | 基础参数校验 | 🟡 低危 |

## 2. 安全分层设计

### 2.1 传输层安全 (Layer 1)

**强制 TLS 1.3**
```go
// 生产环境强制 wss://
// 禁止 ws:// 明文传输
if !strings.HasPrefix(config.Server.Bind, "localhost") {
    requireTLS = true
}
```

**证书配置**
- 生产：使用受信任的 CA 证书
- 开发：支持自签名证书（需显式信任）
- 禁用：SSLv3, TLS 1.0/1.1

**证书固定 (可选强化)**
```go
// 客户端固定服务端证书指纹
expectedFingerprint := "sha256/47DEQpj8HBSa+/TImW+5JCeuQeRkm5NMpJWZG3hSuFU="
```

### 2.2 认证层安全 (Layer 2)

#### 方案 A: 挑战-响应 + HMAC (推荐)

**注册流程 (首次连接)**
```
Desktop                     Daemon
   │    1. 请求挑战           │
   │ ───────────────────────►│
   │    2. 返回随机数+时间戳   │
   │ ◄───────────────────────│
   │    3. HMAC(Token, 挑战)  │
   │ ───────────────────────►│
   │    4. 验证通过，返回 JWT  │
   │ ◄───────────────────────│
```

**实现细节**
```go
// Daemon 端
func handleChallenge(ctx context.Context) (*ChallengeResponse, error) {
    challenge := generateSecureRandom(32)
    expiresAt := time.Now().Add(5 * time.Minute)
    
    // 存储挑战
    challengeStore.Set(challenge, expiresAt)
    
    return &ChallengeResponse{
        Challenge: base64.StdEncoding.EncodeToString(challenge),
        ExpiresAt: expiresAt.Unix(),
        Algorithm: "HMAC-SHA256",
    }, nil
}

func handleAuth(ctx context.Context, req AuthRequest) (*AuthResponse, error) {
    // 验证挑战未过期
    if !challengeStore.Verify(req.Challenge) {
        return nil, ErrChallengeExpired
    }
    
    // 计算期望的 HMAC
    expectedHMAC := hmacSHA256(serverToken, req.Challenge)
    
    // 常量时间比较（防时序攻击）
    if !hmac.Equal(expectedHMAC, req.Signature) {
        return nil, ErrInvalidCredentials
    }
    
    // 生成短期 JWT
    token := generateJWT(sessionID, 1*time.Hour)
    return &AuthResponse{Token: token}, nil
}
```

#### 方案 B: mTLS (双向 TLS)

**证书分发**
```
Daemon 持有: server.crt, server.key
Desktop 持有: client.crt, client.key, ca.crt

连接时双方验证对方证书
```

**优点**: 无需应用层认证，传输层即确认身份
**缺点**: 证书管理复杂，轮换困难

### 2.3 会话层安全 (Layer 3)

**JWT 会话设计**
```go
type Claims struct {
    jwt.RegisteredClaims
    SessionID string   `json:"sid"`
    ClientIP  string   `json:"ip"`
    Permissions []string `json:"perms"`
}

// Token 配置
AccessTokenTTL:  15 * time.Minute
RefreshTokenTTL: 7 * 24 * time.Hour
```

**会话绑定**
```go
// 会话绑定到 IP + 指纹
type Session struct {
    ID        string
    ClientIP  string
    UserAgent string
    CreatedAt time.Time
    LastSeen  time.Time
    
    // 安全：绑定到特定连接
    BoundConnID string
}
```

**心跳与过期**
```go
// 5分钟无心跳自动断开
heartbeatInterval := 30 * time.Second
sessionTimeout := 5 * time.Minute

// 定期清理过期会话
go sessionCleaner()
```

### 2.4 授权层安全 (Layer 4)

**权限分级**
```go
const (
    PermStatusRead  = "status:read"   // 查看状态
    PermStatusWrite = "status:write"  // 启动/停止
    PermConfigRead  = "config:read"   // 读取配置
    PermConfigWrite = "config:write"  // 修改配置
    PermFileRead    = "file:read"     // 下载文件
    PermFileWrite   = "file:write"    // 上传文件（危险）
    PermAdmin       = "admin"         // 所有权限
)

// 默认权限
defaultPerms := []string{
    PermStatusRead,
    PermStatusWrite,
    PermConfigRead,
}
```

**方法级权限检查**
```go
var methodPermissions = map[string][]string{
    "napcat.start":  {PermStatusWrite},
    "napcat.stop":   {PermStatusWrite},
    "config.set":    {PermConfigWrite},
    "file.upload":   {PermFileWrite},
    "file.download": {PermFileRead},
}
```

### 2.5 应用层安全 (Layer 5)

**输入验证**
```go
// 严格参数校验
func validateWorkDir(dir string) error {
    // 禁止目录遍历
    if strings.Contains(dir, "..") {
        return ErrInvalidPath
    }
    
    // 禁止系统目录
    blockedPrefixes := []string{"/etc", "/usr", "/bin", "/sbin", "/lib"}
    for _, prefix := range blockedPrefixes {
        if strings.HasPrefix(dir, prefix) {
            return ErrForbiddenPath
        }
    }
    
    // 路径规范化
    cleanPath := filepath.Clean(dir)
    ...
}
```

**命令注入防护**
```go
// 禁止 shell 解释，使用数组传参
bad:  exec.Command("bash", "-c", userInput)  // ❌
good: exec.Command(binaryPath, arg1, arg2)    // ✅
```

**文件操作沙箱**
```go
// 文件只允许在特定目录
allowedBaseDirs := []string{
    "$HOME/Napcat",
    "/opt/napcat",
}
```

## 3. 安全监控与审计

### 3.1 审计日志

**记录内容**
```go
type AuditLog struct {
    Timestamp   time.Time
    Event       string      // auth.success, auth.failure, napcat.start, etc.
    ClientIP    string
    SessionID   string
    Method      string
    Params      map[string]any  // 脱敏后的参数
    Result      string      // success, failure
    ErrorCode   string
    Duration    time.Duration
}
```

**安全事件**
```
- auth.challenge.requested
- auth.challenge.failed_expired
- auth.success
- auth.failure_invalid_signature
- auth.failure_token_mismatch
- session.created
- session.expired
- session.terminated
- permission.denied
- rate_limit.hit
- command.blocked (危险命令)
```

### 3.2 速率限制

```go
// 认证端点严格限制
authLimiter := rate.NewLimiter(rate.Every(5*time.Second), 3)

// 一般端点宽松限制
generalLimiter := rate.NewLimiter(rate.Every(time.Second), 10)

// 按 IP 隔离
ipLimiter := NewIPRateLimiter()
```

### 3.3 异常检测

```go
// 检测暴力破解
if authFailures[clientIP] > 5 {
    blockIP(clientIP, 15*time.Minute)
    alert("Possible brute force attack from " + clientIP)
}

// 检测异常时间访问
if hour < 6 || hour > 23 {
    logSecurityEvent("off_hours_access", clientIP)
}
```

## 4. 实施优先级

### P0: 必须立即实施 (阻断性风险)

- [ ] **强制 TLS** - 所有生产环境必须使用 wss://
- [ ] **挑战-响应认证** - 替换固定 Token，防止重放攻击
- [ ] **输入验证** - 路径遍历、命令注入防护
- [ ] **安全头部** - CORS, CSP, HSTS

### P1: 尽快实施 (高风险)

- [ ] **JWT 会话管理** - 短期 Token + 刷新机制
- [ ] **会话绑定** - 绑定 IP + 连接
- [ ] **审计日志** - 所有敏感操作记录
- [ ] **速率限制** - 防暴力破解

### P2: 中期实施 (中风险)

- [ ] **权限分级** - 基于角色的访问控制
- [ ] **异常检测** - 自动封禁可疑 IP
- [ ] **日志脱敏** - 自动过滤敏感信息
- [ ] **证书轮换** - 支持热更新证书

### P3: 长期规划 (低风险)

- [ ] **mTLS 支持** - 双向证书认证
- [ ] **硬件安全模块** - HSM 集成
- [ ] **零信任架构** - 持续验证

## 5. 实施建议

### 5.1 最小可用安全 (MVP)

实现以下即可达到基础安全标准：

```
1. TLS 1.3 (wss://)
2. 挑战-响应认证 (HMAC-SHA256)
3. JWT 会话 (15分钟过期)
4. 基础输入验证
5. 审计日志
```

### 5.2 向后兼容

```go
// 支持新旧认证方式过渡
func handleAuth(req AuthRequest) {
    if req.Version == "2" {
        // 新: 挑战-响应
        return handleChallengeAuth(req)
    }
    // 旧: 固定 Token (废弃警告)
    logWarning("Deprecated auth method used")
    return handleLegacyAuth(req)
}
```

## 6. 安全测试清单

- [ ] 中间人攻击测试 (mitmproxy)
- [ ] Token 爆破测试 (hydra)
- [ ] 重放攻击测试
- [ ] 路径遍历测试
- [ ] 命令注入测试
- [ ] 拒绝服务测试
- [ ] 会话固定测试
- [ ] 跨站 WebSocket 劫持测试
