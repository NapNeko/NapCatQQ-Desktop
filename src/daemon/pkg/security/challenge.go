// Package security provides authentication and security utilities
package security

import (
	"crypto/hmac"
	"crypto/rand"
	"crypto/sha256"
	"crypto/subtle"
	"encoding/base64"
	"encoding/hex"
	"fmt"
	"sync"
	"time"
)

// Challenge represents an authentication challenge
type Challenge struct {
	Value     string
	CreatedAt time.Time
	ExpiresAt time.Time
	Used      bool
}

// IsValid checks if the challenge is still valid
func (c *Challenge) IsValid() bool {
	if c.Used {
		return false
	}
	return time.Now().Before(c.ExpiresAt)
}

// ChallengeStore manages authentication challenges
type ChallengeStore struct {
	mu         sync.RWMutex
	challenges map[string]*Challenge
	ttl        time.Duration
}

// NewChallengeStore creates a new challenge store
func NewChallengeStore(ttl time.Duration) *ChallengeStore {
	if ttl <= 0 {
		ttl = 5 * time.Minute
	}

	store := &ChallengeStore{
		challenges: make(map[string]*Challenge),
		ttl:        ttl,
	}

	// Start cleanup goroutine
	go store.cleanupLoop()

	return store
}

// Generate creates a new challenge
func (s *ChallengeStore) Generate() (*Challenge, error) {
	// Generate 32 bytes of random data
	bytes := make([]byte, 32)
	if _, err := rand.Read(bytes); err != nil {
		return nil, fmt.Errorf("failed to generate challenge: %w", err)
	}

	challenge := &Challenge{
		Value:     base64.StdEncoding.EncodeToString(bytes),
		CreatedAt: time.Now(),
		ExpiresAt: time.Now().Add(s.ttl),
		Used:      false,
	}

	s.mu.Lock()
	s.challenges[challenge.Value] = challenge
	s.mu.Unlock()

	return challenge, nil
}

// Verify validates a challenge response
func (s *ChallengeStore) Verify(challengeValue string, signature string, secret string) bool {
	s.mu.Lock()
	defer s.mu.Unlock()

	challenge, exists := s.challenges[challengeValue]
	if !exists {
		return false
	}

	// Mark as used (single-use)
	challenge.Used = true
	delete(s.challenges, challengeValue)

	// Check if expired
	if !challenge.IsValid() {
		return false
	}

	// Calculate expected HMAC
	expectedHMAC := CalculateHMAC(challengeValue, secret)

	// Constant-time comparison to prevent timing attacks
	return subtle.ConstantTimeCompare(
		[]byte(expectedHMAC),
		[]byte(signature),
	) == 1
}

// Cleanup removes expired challenges
func (s *ChallengeStore) Cleanup() {
	s.mu.Lock()
	defer s.mu.Unlock()

	now := time.Now()
	for key, challenge := range s.challenges {
		if now.After(challenge.ExpiresAt) || challenge.Used {
			delete(s.challenges, key)
		}
	}
}

func (s *ChallengeStore) cleanupLoop() {
	ticker := time.NewTicker(1 * time.Minute)
	defer ticker.Stop()

	for range ticker.C {
		s.Cleanup()
	}
}

// CalculateHMAC calculates HMAC-SHA256 of data with secret
func CalculateHMAC(data, secret string) string {
	h := hmac.New(sha256.New, []byte(secret))
	h.Write([]byte(data))
	return hex.EncodeToString(h.Sum(nil))
}

// GenerateSecureToken generates a cryptographically secure random token
func GenerateSecureToken(length int) (string, error) {
	if length <= 0 {
		length = 32
	}

	bytes := make([]byte, length)
	if _, err := rand.Read(bytes); err != nil {
		return "", err
	}

	return base64.URLEncoding.EncodeToString(bytes), nil
}

// SecureCompare performs constant-time string comparison
func SecureCompare(a, b string) bool {
	return subtle.ConstantTimeCompare([]byte(a), []byte(b)) == 1
}
