// Package security provides rate limiting utilities
package security

import (
	"net"
	"sync"
	"time"
)

// RateLimiter provides token bucket rate limiting
type RateLimiter struct {
	mu       sync.Mutex
	tokens   float64
	capacity float64
	rate     float64
	lastTime time.Time
}

// NewRateLimiter creates a new rate limiter
func NewRateLimiter(rate, capacity float64) *RateLimiter {
	return &RateLimiter{
		tokens:   capacity,
		capacity: capacity,
		rate:     rate,
		lastTime: time.Now(),
	}
}

// Allow checks if a request is allowed
func (r *RateLimiter) Allow() bool {
	r.mu.Lock()
	defer r.mu.Unlock()

	// Add tokens based on time elapsed
	now := time.Now()
	elapsed := now.Sub(r.lastTime).Seconds()
	r.tokens += elapsed * r.rate
	if r.tokens > r.capacity {
		r.tokens = r.capacity
	}
	r.lastTime = now

	if r.tokens >= 1 {
		r.tokens--
		return true
	}
	return false
}

// WaitTime returns how long to wait before the next request is allowed
func (r *RateLimiter) WaitTime() time.Duration {
	r.mu.Lock()
	defer r.mu.Unlock()

	if r.tokens >= 1 {
		return 0
	}

	needed := 1 - r.tokens
	seconds := needed / r.rate
	return time.Duration(seconds * float64(time.Second))
}

// IPRateLimiter provides per-IP rate limiting
type IPRateLimiter struct {
	mu       sync.RWMutex
	limiters map[string]*RateLimiter
	rate     float64
	capacity float64
	ttl      time.Duration
}

// NewIPRateLimiter creates a new per-IP rate limiter
func NewIPRateLimiter(rate, capacity float64) *IPRateLimiter {
	if rate <= 0 {
		rate = 1 // 1 per second default
	}
	if capacity <= 0 {
		capacity = 5 // burst of 5 default
	}

	limiter := &IPRateLimiter{
		limiters: make(map[string]*RateLimiter),
		rate:     rate,
		capacity: capacity,
		ttl:      1 * time.Hour,
	}

	go limiter.cleanupLoop()
	return limiter
}

// Allow checks if a request from the given IP is allowed
func (r *IPRateLimiter) Allow(ip string) bool {
	r.mu.Lock()
	limiter, exists := r.limiters[ip]
	if !exists {
		limiter = NewRateLimiter(r.rate, r.capacity)
		r.limiters[ip] = limiter
	}
	r.mu.Unlock()

	return limiter.Allow()
}

// GetLimiter returns the rate limiter for a specific IP
func (r *IPRateLimiter) GetLimiter(ip string) *RateLimiter {
	r.mu.Lock()
	defer r.mu.Unlock()

	limiter, exists := r.limiters[ip]
	if !exists {
		limiter = NewRateLimiter(r.rate, r.capacity)
		r.limiters[ip] = limiter
	}
	return limiter
}

// cleanupLoop periodically removes old limiters
func (r *IPRateLimiter) cleanupLoop() {
	ticker := time.NewTicker(10 * time.Minute)
	defer ticker.Stop()

	for range ticker.C {
		r.Cleanup()
	}
}

// Cleanup removes all limiters (call periodically)
func (r *IPRateLimiter) Cleanup() {
	r.mu.Lock()
	defer r.mu.Unlock()

	// In a real implementation, track last access time
	// For now, we keep limiters to prevent memory leak from rotating IPs
	if len(r.limiters) > 10000 {
		// Reset if too many
		r.limiters = make(map[string]*RateLimiter)
	}
}

// AuthRateLimiter provides stricter rate limiting for auth endpoints
type AuthRateLimiter struct {
	attempts   map[string]*AuthAttempts
	mu         sync.RWMutex
	maxAttempts int
	window     time.Duration
	blockDuration time.Duration
}

// AuthAttempts tracks failed auth attempts
type AuthAttempts struct {
	Count      int
	FirstAttempt time.Time
	LastAttempt  time.Time
	BlockedUntil *time.Time
}

// NewAuthRateLimiter creates an auth-specific rate limiter
func NewAuthRateLimiter() *AuthRateLimiter {
	return &AuthRateLimiter{
		attempts:       make(map[string]*AuthAttempts),
		maxAttempts:    5,
		window:         15 * time.Minute,
		blockDuration:  15 * time.Minute,
	}
}

// RecordAttempt records an auth attempt
func (a *AuthRateLimiter) RecordAttempt(identifier string, success bool) bool {
	a.mu.Lock()
	defer a.mu.Unlock()

	now := time.Now()
	attempt, exists := a.attempts[identifier]

	if !exists {
		if success {
			return true
		}
		a.attempts[identifier] = &AuthAttempts{
			Count:        1,
			FirstAttempt: now,
			LastAttempt:  now,
		}
		return true
	}

	// Check if blocked
	if attempt.BlockedUntil != nil && now.Before(*attempt.BlockedUntil) {
		return false
	}

	// Reset if window expired
	if now.Sub(attempt.FirstAttempt) > a.window {
		attempt.Count = 0
		attempt.FirstAttempt = now
		attempt.BlockedUntil = nil
	}

	if success {
		// Clear attempts on success
		delete(a.attempts, identifier)
		return true
	}

	// Record failure
	attempt.Count++
	attempt.LastAttempt = now

	// Block if too many attempts
	if attempt.Count >= a.maxAttempts {
		blockUntil := now.Add(a.blockDuration)
		attempt.BlockedUntil = &blockUntil
		return false
	}

	return true
}

// IsBlocked checks if an identifier is currently blocked
func (a *AuthRateLimiter) IsBlocked(identifier string) bool {
	a.mu.RLock()
	defer a.mu.RUnlock()

	attempt, exists := a.attempts[identifier]
	if !exists {
		return false
	}

	if attempt.BlockedUntil == nil {
		return false
	}

	return time.Now().Before(*attempt.BlockedUntil)
}

// GetRemainingAttempts returns remaining attempts before block
func (a *AuthRateLimiter) GetRemainingAttempts(identifier string) int {
	a.mu.RLock()
	defer a.mu_RUnlock()

	attempt, exists := a.attempts[identifier]
	if !exists {
		return a.maxAttempts
	}

	if attempt.BlockedUntil != nil {
		return 0
	}

	remaining := a.maxAttempts - attempt.Count
	if remaining < 0 {
		return 0
	}
	return remaining
}

// Cleanup removes old entries
func (a *AuthRateLimiter) Cleanup() {
	a.mu.Lock()
	defer a.mu.Unlock()

	now := time.Now()
	for id, attempt := range a.attempts {
		// Remove if block expired and window passed
		if attempt.BlockedUntil != nil && now.After(*attempt.BlockedUntil) {
			if now.Sub(attempt.LastAttempt) > a.window {
				delete(a.attempts, id)
			}
		}
	}
}

// ExtractIP extracts the client IP from a network address
func ExtractIP(addr string) string {
	host, _, err := net.SplitHostPort(addr)
	if err != nil {
		return addr
	}
	return host
}

// ConnectionLimiter limits concurrent connections per IP
type ConnectionLimiter struct {
	mu          sync.RWMutex
	connections map[string]int
	maxConn     int
}

// NewConnectionLimiter creates a connection limiter
func NewConnectionLimiter(maxConn int) *ConnectionLimiter {
	if maxConn <= 0 {
		maxConn = 10
	}
	return &ConnectionLimiter{
		connections: make(map[string]int),
		maxConn:     maxConn,
	}
}

// Acquire attempts to acquire a connection slot
func (c *ConnectionLimiter) Acquire(ip string) bool {
	c.mu.Lock()
	defer c.mu.Unlock()

	if c.connections[ip] >= c.maxConn {
		return false
	}
	c.connections[ip]++
	return true
}

// Release releases a connection slot
func (c *ConnectionLimiter) Release(ip string) {
	c.mu.Lock()
	defer c.mu.Unlock()

	if c.connections[ip] > 0 {
		c.connections[ip]--
		if c.connections[ip] == 0 {
			delete(c.connections, ip)
		}
	}
}

// GetCount returns the current connection count for an IP
func (c *ConnectionLimiter) GetCount(ip string) int {
	c.mu.RLock()
	defer c.mu.RUnlock()
	return c.connections[ip]
}
