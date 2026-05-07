// Package security provides audit logging capabilities
package security

import (
	"encoding/json"
	"fmt"
	"log"
	"os"
	"path/filepath"
	"sync"
	"time"
)

// AuditEventType represents the type of audit event
type AuditEventType string

const (
	EventAuthChallenge      AuditEventType = "auth.challenge.requested"
	EventAuthChallengeFail  AuditEventType = "auth.challenge.failed_expired"
	EventAuthSuccess        AuditEventType = "auth.success"
	EventAuthFailSignature  AuditEventType = "auth.failure.invalid_signature"
	EventAuthFailToken      AuditEventType = "auth.failure.token_mismatch"
	EventAuthFailExpired    AuditEventType = "auth.failure.token_expired"
	EventSessionCreate      AuditEventType = "session.created"
	EventSessionExpire      AuditEventType = "session.expired"
	EventSessionTerminate   AuditEventType = "session.terminated"
	EventSessionInvalidIP   AuditEventType = "session.invalid_ip"
	EventPermissionDenied   AuditEventType = "permission.denied"
	EventRateLimitHit       AuditEventType = "rate_limit.hit"
	EventCommandBlocked     AuditEventType = "command.blocked"
	EventFileAccessBlocked  AuditEventType = "file.access_blocked"
	EventPathTraversal      AuditEventType = "security.path_traversal_attempt"
	EventConnLimitHit       AuditEventType = "connection.limit_hit"

	// NapCat specific events
	EventNapCatStart   AuditEventType = "napcat.start"
	EventNapCatStop    AuditEventType = "napcat.stop"
	EventNapCatRestart AuditEventType = "napcat.restart"
	EventConfigGet     AuditEventType = "config.get"
	EventConfigSet     AuditEventType = "config.set"
	EventFileUpload    AuditEventType = "file.upload"
	EventFileDownload  AuditEventType = "file.download"
)

// Severity represents the severity level of an event
type Severity string

const (
	SeverityDebug   Severity = "DEBUG"
	SeverityInfo    Severity = "INFO"
	SeverityWarning Severity = "WARNING"
	SeverityError   Severity = "ERROR"
	SeverityCritical Severity = "CRITICAL"
)

// AuditEntry represents a single audit log entry
type AuditEntry struct {
	Timestamp   time.Time       `json:"timestamp"`
	EventType   AuditEventType  `json:"event_type"`
	Severity    Severity        `json:"severity"`
	ClientIP    string          `json:"client_ip,omitempty"`
	SessionID   string          `json:"session_id,omitempty"`
	UserAgent   string          `json:"user_agent,omitempty"`
	Method      string          `json:"method,omitempty"`
	Params      map[string]any  `json:"params,omitempty"`
	Result      string          `json:"result,omitempty"`
	ErrorCode   string          `json:"error_code,omitempty"`
	ErrorMessage string         `json:"error_message,omitempty"`
	Duration    time.Duration   `json:"duration,omitempty"`
	Details     map[string]any  `json:"details,omitempty"`
}

// AuditLogger handles security audit logging
type AuditLogger struct {
	mu         sync.RWMutex
	writer     *os.File
	encoder    *json.Encoder
	minSeverity Severity
	sink       chan *AuditEntry
}

// NewAuditLogger creates a new audit logger
func NewAuditLogger(logPath string, minSeverity Severity) (*AuditLogger, error) {
	if minSeverity == "" {
		minSeverity = SeverityInfo
	}

	// Ensure directory exists
	dir := filepath.Dir(logPath)
	if err := os.MkdirAll(dir, 0750); err != nil {
		return nil, fmt.Errorf("failed to create audit log directory: %w", err)
	}

	// Open log file with secure permissions
	file, err := os.OpenFile(logPath, os.O_CREATE|os.O_APPEND|os.O_WRONLY, 0600)
	if err != nil {
		return nil, fmt.Errorf("failed to open audit log: %w", err)
	}

	logger := &AuditLogger{
		writer:      file,
		encoder:     json.NewEncoder(file),
		minSeverity: minSeverity,
		sink:        make(chan *AuditEntry, 1000),
	}

	// Start background writer
	go logger.writeLoop()

	return logger, nil
}

// Log records an audit event
func (a *AuditLogger) Log(entry *AuditEntry) {
	if entry == nil {
		return
	}

	// Set timestamp if not set
	if entry.Timestamp.IsZero() {
		entry.Timestamp = time.Now()
	}

	// Filter by severity
	if !a.shouldLog(entry.Severity) {
		return
	}

	// Sanitize sensitive data
	entry.Params = sanitizeParams(entry.Params)

	// Send to channel (non-blocking)
	select {
	case a.sink <- entry:
	default:
		// Channel full, log warning
		log.Printf("Warning: Audit log channel full, dropping event: %s", entry.EventType)
	}
}

// LogAuthChallenge logs an auth challenge request
func (a *AuditLogger) LogAuthChallenge(clientIP string, success bool) {
	event := EventAuthChallenge
	severity := SeverityInfo
	result := "success"

	if !success {
		event = EventAuthChallengeFail
		severity = SeverityWarning
		result = "failed"
	}

	a.Log(&AuditEntry{
		EventType: event,
		Severity:  severity,
		ClientIP:  clientIP,
		Result:    result,
	})
}

// LogAuth logs an authentication attempt
func (a *AuditLogger) LogAuth(clientIP, sessionID string, success bool, err error) {
	event := EventAuthSuccess
	severity := SeverityInfo
	result := "success"
	var errCode, errMsg string

	if !success {
		if err != nil {
			errMsg = err.Error()
			switch err.Error() {
			case "invalid signature":
				event = EventAuthFailSignature
			case "token expired":
				event = EventAuthFailExpired
			default:
				event = EventAuthFailToken
			}
		} else {
			event = EventAuthFailToken
		}
		severity = SeverityWarning
		result = "failure"
	}

	a.Log(&AuditEntry{
		EventType:    event,
		Severity:     severity,
		ClientIP:     clientIP,
		SessionID:    sessionID,
		Result:       result,
		ErrorCode:    errCode,
		ErrorMessage: errMsg,
	})
}

// LogSession logs a session event
func (a *AuditLogger) LogSession(event AuditEventType, clientIP, sessionID string, details map[string]any) {
	severity := SeverityInfo
	if event == EventSessionInvalidIP {
		severity = SeverityWarning
	}

	a.Log(&AuditEntry{
		EventType: event,
		Severity:  severity,
		ClientIP:  clientIP,
		SessionID: sessionID,
		Details:   details,
	})
}

// LogPermissionDenied logs a permission denial
func (a *AuditLogger) LogPermissionDenied(clientIP, sessionID, method string, requiredPerm string) {
	a.Log(&AuditEntry{
		EventType: EventPermissionDenied,
		Severity:  SeverityWarning,
		ClientIP:  clientIP,
		SessionID: sessionID,
		Method:    method,
		Details: map[string]any{
			"required_permission": requiredPerm,
		},
	})
}

// LogRateLimit logs a rate limit hit
func (a *AuditLogger) LogRateLimit(clientIP, limitType string) {
	a.Log(&AuditEntry{
		EventType: EventRateLimitHit,
		Severity:  SeverityWarning,
		ClientIP:  clientIP,
		Details: map[string]any{
			"limit_type": limitType,
		},
	})
}

// LogSecurityEvent logs a general security event
func (a *AuditLogger) LogSecurityEvent(event AuditEventType, clientIP string, details map[string]any) {
	severity := SeverityWarning
	if event == EventPathTraversal || event == EventCommandBlocked {
		severity = SeverityError
	}

	a.Log(&AuditEntry{
		EventType: event,
		Severity:  severity,
		ClientIP:  clientIP,
		Details:   details,
	})
}

// LogMethodCall logs a method invocation
func (a *AuditLogger) LogMethodCall(clientIP, sessionID, method string, params map[string]any, duration time.Duration, err error) {
	entry := &AuditEntry{
		EventType: AuditEventType(fmt.Sprintf("method.%s", method)),
		Severity:  SeverityInfo,
		ClientIP:  clientIP,
		SessionID: sessionID,
		Method:    method,
		Params:    params,
		Duration:  duration,
	}

	if err != nil {
		entry.Result = "error"
		entry.ErrorMessage = err.Error()
		entry.Severity = SeverityWarning
	} else {
		entry.Result = "success"
	}

	a.Log(entry)
}

// shouldLog checks if the severity meets the minimum threshold
func (a *AuditLogger) shouldLog(severity Severity) bool {
	levels := map[Severity]int{
		SeverityDebug:    0,
		SeverityInfo:     1,
		SeverityWarning:  2,
		SeverityError:    3,
		SeverityCritical: 4,
	}

	return levels[severity] >= levels[a.minSeverity]
}

// writeLoop processes audit entries in the background
func (a *AuditLogger) writeLoop() {
	for entry := range a.sink {
		a.mu.Lock()
		if err := a.encoder.Encode(entry); err != nil {
			log.Printf("Failed to write audit log: %v", err)
		}
		a.mu.Unlock()
	}
}

// Close closes the audit logger
func (a *AuditLogger) Close() error {
	close(a.sink)
	a.mu.Lock()
	defer a.mu.Unlock()
	return a.writer.Close()
}

// Rotate rotates the audit log file
func (a *AuditLogger) Rotate(newPath string) error {
	a.mu.Lock()
	defer a.mu.Unlock()

	// Close old file
	if err := a.writer.Close(); err != nil {
		return err
	}

	// Open new file
	file, err := os.OpenFile(newPath, os.O_CREATE|os.O_APPEND|os.O_WRONLY, 0600)
	if err != nil {
		return err
	}

	a.writer = file
	a.encoder = json.NewEncoder(file)

	return nil
}

// sanitizeParams removes sensitive data from params
func sanitizeParams(params map[string]any) map[string]any {
	if params == nil {
		return nil
	}

	sensitiveKeys := []string{"token", "password", "secret", "key", "signature", "content"}
	sanitized := make(map[string]any)

	for k, v := range params {
		isSensitive := false
		for _, sk := range sensitiveKeys {
			if k == sk {
				isSensitive = true
				break
			}
		}

		if isSensitive {
			sanitized[k] = "[REDACTED]"
		} else {
			sanitized[k] = v
		}
	}

	return sanitized
}

// ConsoleAlert prints critical security events to stderr
func ConsoleAlert(event AuditEventType, details string) {
	fmt.Fprintf(os.Stderr, "\n[SECURITY ALERT] %s at %s\n", event, time.Now().Format(time.RFC3339))
	fmt.Fprintf(os.Stderr, "Details: %s\n\n", details)
}
