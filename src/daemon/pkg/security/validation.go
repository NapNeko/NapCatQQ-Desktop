// Package security provides input validation utilities
package security

import (
	"fmt"
	"net"
	"path/filepath"
	"regexp"
	"strings"
)

// Validator provides input validation functions
type Validator struct {
	allowedBaseDirs []string
	blockedPaths    []string
}

// NewValidator creates a new validator
func NewValidator(allowedBaseDirs []string) *Validator {
	if len(allowedBaseDirs) == 0 {
		allowedBaseDirs = []string{"$HOME/Napcat", "/opt/napcat"}
	}

	return &Validator{
		allowedBaseDirs: allowedBaseDirs,
		blockedPaths: []string{
			"/etc", "/usr", "/bin", "/sbin", "/lib", "/lib64",
			"/boot", "/dev", "/proc", "/sys", "/root",
			"/var/log", "/var/lib", "/var/spool",
		},
	}
}

// ValidatePath validates a file path for security
func (v *Validator) ValidatePath(path string) error {
	if path == "" {
		return fmt.Errorf("path cannot be empty")
	}

	// Check for null bytes
	if strings.Contains(path, "\x00") {
		return fmt.Errorf("path contains null bytes")
	}

	// Check for directory traversal
	if strings.Contains(path, "..") {
		return fmt.Errorf("path contains directory traversal sequence")
	}

	// Clean the path
	cleanPath := filepath.Clean(path)

	// Check for absolute vs relative
	if !filepath.IsAbs(cleanPath) {
		return fmt.Errorf("path must be absolute")
	}

	// Check blocked paths
	for _, blocked := range v.blockedPaths {
		if strings.HasPrefix(cleanPath, blocked) {
			return fmt.Errorf("path is in blocked directory: %s", blocked)
		}
	}

	// Check allowed base directories
	allowed := false
	for _, base := range v.allowedBaseDirs {
		baseClean := filepath.Clean(base)
		if strings.HasPrefix(cleanPath, baseClean) {
			allowed = true
			break
		}
	}

	if !allowed {
		return fmt.Errorf("path is outside allowed directories")
	}

	return nil
}

// ValidateWorkDir validates a working directory
func (v *Validator) ValidateWorkDir(dir string) error {
	// Work dir can be relative, but must not traverse
	if strings.Contains(dir, "..") {
		return fmt.Errorf("work directory cannot contain parent references")
	}

	// Must not start with /
	if strings.HasPrefix(dir, "/") && !strings.HasPrefix(dir, "$HOME") {
		// Check if it's in allowed base
		allowed := false
		for _, base := range v.allowedBaseDirs {
			if strings.HasPrefix(dir, base) {
				allowed = true
				break
			}
		}
		if !allowed {
			return fmt.Errorf("work directory must be under $HOME or allowed base")
		}
	}

	return nil
}

// ValidateIP validates an IP address
func (v *Validator) ValidateIP(ip string) error {
	if ip == "" {
		return fmt.Errorf("IP cannot be empty")
	}

	parsedIP := net.ParseIP(ip)
	if parsedIP == nil {
		return fmt.Errorf("invalid IP address format")
	}

	// Check for private IPs (warning only)
	if parsedIP.IsLoopback() || parsedIP.IsPrivate() {
		// This is OK, just note it
	}

	return nil
}

// ValidatePort validates a port number
func (v *Validator) ValidatePort(port int) error {
	if port < 1 || port > 65535 {
		return fmt.Errorf("port must be between 1 and 65535")
	}

	// Check well-known ports (require additional privileges)
	if port < 1024 {
		return fmt.Errorf("well-known ports (<1024) require special privileges")
	}

	return nil
}

// ValidateToken validates a token format
func (v *Validator) ValidateToken(token string) error {
	if token == "" {
		return fmt.Errorf("token cannot be empty")
	}

	if len(token) < 16 {
		return fmt.Errorf("token too short (minimum 16 characters)")
	}

	// Check for common weak tokens
	weakTokens := []string{"password", "123456", "admin", "token", "secret"}
	lowerToken := strings.ToLower(token)
	for _, weak := range weakTokens {
		if lowerToken == weak {
			return fmt.Errorf("token is too common/weak")
		}
	}

	return nil
}

// ValidateCommand validates a command string (for logging, not execution)
func (v *Validator) ValidateCommand(cmd string) error {
	if cmd == "" {
		return fmt.Errorf("command cannot be empty")
	}

	// Check for shell metacharacters if this were to be executed
	dangerousChars := []string{";", "&", "|", "`", "$", "(", ")", "<", ">"}
	for _, char := range dangerousChars {
		if strings.Contains(cmd, char) {
			return fmt.Errorf("command contains dangerous characters")
		}
	}

	return nil
}

// SanitizeString sanitizes a string for logging
func (v *Validator) SanitizeString(s string) string {
	// Remove control characters
	var result strings.Builder
	for _, r := range s {
		if r >= 32 && r < 127 {
			result.WriteRune(r)
		} else if r == '\n' || r == '\t' {
			result.WriteRune(r)
		} else {
			result.WriteRune('?')
		}
	}
	return result.String()
}

// TruncateString truncates a string to max length
func TruncateString(s string, maxLen int) string {
	if len(s) <= maxLen {
		return s
	}
	return s[:maxLen] + "..."
}

// IsSafeFileName checks if a filename is safe
func IsSafeFileName(name string) bool {
	if name == "" {
		return false
	}

	// Check for path separators
	if strings.ContainsAny(name, "/\\") {
		return false
	}

	// Check for dangerous patterns
	dangerous := []string{"..", "~", "$", "`", "|", ";", "&"}
	for _, d := range dangerous {
		if strings.Contains(name, d) {
			return false
		}
	}

	// Check for valid characters
	validName := regexp.MustCompile(`^[a-zA-Z0-9._-]+$`)
	return validName.MatchString(name)
}

// AllowedFileExtensions returns safe file extensions for uploads
func AllowedFileExtensions() []string {
	return []string{
		".json", ".yaml", ".yml", ".conf", ".config",
		".txt", ".log",
		".sh", ".py", ".js",
	}
}

// ValidateFileExtension checks if a file extension is allowed
func ValidateFileExtension(filename string) error {
	ext := strings.ToLower(filepath.Ext(filename))

	allowed := AllowedFileExtensions()
	for _, a := range allowed {
		if ext == a {
			return nil
		}
	}

	return fmt.Errorf("file extension %s not allowed", ext)
}

// MaxFileSize returns the maximum allowed file size
func MaxFileSize() int64 {
	return 10 * 1024 * 1024 // 10 MB
}

// ValidateFileSize checks if a file size is within limits
func ValidateFileSize(size int64) error {
	maxSize := MaxFileSize()
	if size > maxSize {
		return fmt.Errorf("file size %d exceeds maximum %d", size, maxSize)
	}
	return nil
}
