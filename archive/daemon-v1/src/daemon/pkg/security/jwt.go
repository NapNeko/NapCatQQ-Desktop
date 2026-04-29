// Package security provides JWT session management
package security

import (
	"crypto/rand"
	"encoding/base64"
	"encoding/json"
	"fmt"
	"sync"
	"time"
)

// Claims represents JWT claims
type Claims struct {
	// Standard claims
	Subject   string `json:"sub"`
	IssuedAt  int64  `json:"iat"`
	ExpiresAt int64  `json:"exp"`
	NotBefore int64  `json:"nbf"`
	Issuer    string `json:"iss"`
	Audience  string `json:"aud"`
	JWTID     string `json:"jti"`

	// Custom claims
	SessionID   string   `json:"sid"`
	ClientIP    string   `json:"ip"`
	Permissions []string `json:"perms"`
}

// IsExpired checks if the token is expired
func (c *Claims) IsExpired() bool {
	return time.Now().Unix() > c.ExpiresAt
}

// IsValid checks if the token is valid (not expired and not before valid time)
func (c *Claims) IsValid() bool {
	now := time.Now().Unix()
	return now >= c.NotBefore && now <= c.ExpiresAt
}

// HasPermission checks if the claims have a specific permission
func (c *Claims) HasPermission(perm string) bool {
	for _, p := range c.Permissions {
		if p == perm {
			return true
		}
	}
	return false
}

// JWTSigner handles JWT signing and verification
type JWTSigner struct {
	secret     []byte
	issuer     string
	audience   string
	accessTTL  time.Duration
	refreshTTL time.Duration
}

// NewJWTSigner creates a new JWT signer
func NewJWTSigner(secret string, issuer, audience string) *JWTSigner {
	return &JWTSigner{
		secret:     []byte(secret),
		issuer:     issuer,
		audience:   audience,
		accessTTL:  15 * time.Minute,  // Short-lived access tokens
		refreshTTL: 7 * 24 * time.Hour, // Long-lived refresh tokens
	}
}

// SetTTL configures token TTLs
func (j *JWTSigner) SetTTL(access, refresh time.Duration) {
	j.accessTTL = access
	j.refreshTTL = refresh
}

// GenerateAccessToken creates a new access token
func (j *JWTSigner) GenerateAccessToken(sessionID, clientIP string, permissions []string) (string, error) {
	return j.generateToken(sessionID, clientIP, permissions, j.accessTTL)
}

// GenerateRefreshToken creates a new refresh token
func (j *JWTSigner) GenerateRefreshToken(sessionID string) (string, error) {
	return j.generateToken(sessionID, "", nil, j.refreshTTL)
}

func (j *JWTSigner) generateToken(sessionID, clientIP string, permissions []string, ttl time.Duration) (string, error) {
	now := time.Now()

	jti, err := generateJWTID()
	if err != nil {
		return "", err
	}

	claims := Claims{
		Subject:     sessionID,
		IssuedAt:    now.Unix(),
		ExpiresAt:   now.Add(ttl).Unix(),
		NotBefore:   now.Unix(),
		Issuer:      j.issuer,
		Audience:    j.audience,
		JWTID:       jti,
		SessionID:   sessionID,
		ClientIP:    clientIP,
		Permissions: permissions,
	}

	return j.sign(claims)
}

// ParseAndVerify validates a token and returns the claims
func (j *JWTSigner) ParseAndVerify(token string) (*Claims, error) {
	claims, err := j.parse(token)
	if err != nil {
		return nil, err
	}

	// Validate standard claims
	if claims.Issuer != j.issuer {
		return nil, fmt.Errorf("invalid issuer")
	}

	if claims.Audience != j.audience {
		return nil, fmt.Errorf("invalid audience")
	}

	if claims.IsExpired() {
		return nil, fmt.Errorf("token expired")
	}

	if !claims.IsValid() {
		return nil, fmt.Errorf("token not yet valid")
	}

	return claims, nil
}

// Simple JWT implementation (header.payload.signature)
func (j *JWTSigner) sign(claims Claims) (string, error) {
	// Header
	header := map[string]string{
		"alg": "HS256",
		"typ": "JWT",
	}
	headerJSON, err := json.Marshal(header)
	if err != nil {
		return "", err
	}

	// Claims
	claimsJSON, err := json.Marshal(claims)
	if err != nil {
		return "", err
	}

	// Encode
	headerB64 := base64.RawURLEncoding.EncodeToString(headerJSON)
	claimsB64 := base64.RawURLEncoding.EncodeToString(claimsJSON)

	// Sign
	signingInput := headerB64 + "." + claimsB64
	signature := CalculateHMAC(signingInput, string(j.secret))

	return signingInput + "." + signature, nil
}

func (j *JWTSigner) parse(token string) (*Claims, error) {
	parts := splitToken(token)
	if len(parts) != 3 {
		return nil, fmt.Errorf("invalid token format")
	}

	// Verify signature
	signingInput := parts[0] + "." + parts[1]
	expectedSig := CalculateHMAC(signingInput, string(j.secret))

	if !SecureCompare(parts[2], expectedSig) {
		return nil, fmt.Errorf("invalid signature")
	}

	// Decode claims
	claimsJSON, err := base64.RawURLEncoding.DecodeString(parts[1])
	if err != nil {
		return nil, fmt.Errorf("failed to decode claims: %w", err)
	}

	var claims Claims
	if err := json.Unmarshal(claimsJSON, &claims); err != nil {
		return nil, fmt.Errorf("failed to parse claims: %w", err)
	}

	return &claims, nil
}

func splitToken(token string) []string {
	var parts []string
	start := 0
	for i := 0; i < len(token); i++ {
		if token[i] == '.' {
			parts = append(parts, token[start:i])
			start = i + 1
		}
	}
	parts = append(parts, token[start:])
	return parts
}

func generateJWTID() (string, error) {
	bytes := make([]byte, 16)
	if _, err := rand.Read(bytes); err != nil {
		return "", err
	}
	return base64.RawURLEncoding.EncodeToString(bytes), nil
}

// Session represents an authenticated session
type Session struct {
	ID          string
	ClientID    string
	ClientIP    string
	Permissions []string
	CreatedAt   time.Time
	LastSeen    time.Time
	ExpiresAt   time.Time
}

// IsExpired checks if the session is expired
func (s *Session) IsExpired() bool {
	return time.Now().After(s.ExpiresAt)
}

// Touch updates the last seen timestamp
func (s *Session) Touch() {
	s.LastSeen = time.Now()
}

// SessionManager manages authenticated sessions
type SessionManager struct {
	mu       sync.RWMutex
	sessions map[string]*Session
	ttl      time.Duration
}

// NewSessionManager creates a new session manager
func NewSessionManager(ttl time.Duration) *SessionManager {
	if ttl <= 0 {
		ttl = 1 * time.Hour
	}

	manager := &SessionManager{
		sessions: make(map[string]*Session),
		ttl:      ttl,
	}

	go manager.cleanupLoop()
	return manager
}

// Create creates a new session
func (m *SessionManager) Create(clientID, clientIP string, permissions []string) *Session {
	sessionID, _ := GenerateSecureToken(16)

	now := time.Now()
	session := &Session{
		ID:          sessionID,
		ClientID:    clientID,
		ClientIP:    clientIP,
		Permissions: permissions,
		CreatedAt:   now,
		LastSeen:    now,
		ExpiresAt:   now.Add(m.ttl),
	}

	m.mu.Lock()
	m.sessions[sessionID] = session
	m.mu.Unlock()

	return session
}

// Get retrieves a session by ID
func (m *SessionManager) Get(sessionID string) (*Session, bool) {
	m.mu.RLock()
	defer m.mu.RUnlock()

	session, exists := m.sessions[sessionID]
	if !exists {
		return nil, false
	}

	if session.IsExpired() {
		return nil, false
	}

	return session, true
}

// Delete removes a session
func (m *SessionManager) Delete(sessionID string) {
	m.mu.Lock()
	defer m.mu.Unlock()
	delete(m.sessions, sessionID)
}

// Touch updates the last seen timestamp of a session
func (m *SessionManager) Touch(sessionID string) bool {
	m.mu.Lock()
	defer m.mu.Unlock()

	session, exists := m.sessions[sessionID]
	if !exists {
		return false
	}

	if session.IsExpired() {
		delete(m.sessions, sessionID)
		return false
	}

	session.LastSeen = time.Now()
	return true
}

// cleanupLoop periodically removes expired sessions
func (m *SessionManager) cleanupLoop() {
	ticker := time.NewTicker(5 * time.Minute)
	defer ticker.Stop()

	for range ticker.C {
		m.Cleanup()
	}
}

// Cleanup removes all expired sessions
func (m *SessionManager) Cleanup() {
	m.mu.Lock()
	defer m.mu.Unlock()

	now := time.Now()
	for id, session := range m.sessions {
		if now.After(session.ExpiresAt) {
			delete(m.sessions, id)
		}
	}
}

// Count returns the number of active sessions
func (m *SessionManager) Count() int {
	m.mu.RLock()
	defer m.mu.RUnlock()
	return len(m.sessions)
}

// Permission constants
const (
	PermStatusRead  = "status:read"
	PermStatusWrite = "status:write"
	PermConfigRead  = "config:read"
	PermConfigWrite = "config:write"
	PermFileRead    = "file:read"
	PermFileWrite   = "file:write"
	PermAdmin       = "admin"
)

// DefaultPermissions returns the default permission set
func DefaultPermissions() []string {
	return []string{
		PermStatusRead,
		PermStatusWrite,
		PermConfigRead,
	}
}

// AllPermissions returns all available permissions
func AllPermissions() []string {
	return []string{
		PermStatusRead,
		PermStatusWrite,
		PermConfigRead,
		PermConfigWrite,
		PermFileRead,
		PermFileWrite,
		PermAdmin,
	}
}
